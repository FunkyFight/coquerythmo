//! Durable Comic Dubs document: ordered pages, media and translated bubbles.

use crate::recording::{RecordedAudio, WaveformData};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

pub type PageId = u64;
pub type BubbleId = u64;
pub type ComicAudioId = u64;
pub type ShotId = u64;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlignment {
    Left,
    #[default]
    Center,
    Right,
}

pub(crate) fn bubble_playback_state(
    bubble: &Bubble,
    index: usize,
    visible_bubbles: usize,
) -> (bool, bool) {
    let revealed = index < visible_bubbles;
    let has_text = !bubble.text.trim().is_empty();
    (!revealed || has_text, revealed && has_text)
}

/// Cycles through a closed list of choices; used by every studio selector so
/// mouse arrows and keyboard activation step through the same order.
pub fn cycle_choice<T: Copy + PartialEq>(all: &[T], current: T, delta: isize) -> T {
    let Some(index) = all.iter().position(|value| *value == current) else {
        return all.first().copied().unwrap_or(current);
    };
    let len = all.len() as isize;
    all[((index as isize + delta).rem_euclid(len)) as usize]
}

macro_rules! studio_choice {
    ($(#[$meta:meta])* $name:ident { $($(#[$variant_meta:meta])* $variant:ident => $label:expr),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name {
            $($(#[$variant_meta])* $variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// French display name shown by the studio inspector.
            pub fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label),+
                }
            }

            pub fn cycled(self, delta: isize) -> Self {
                cycle_choice(Self::ALL, self, delta)
            }
        }
    };
}

studio_choice!(
    /// How the text (or the whole bubble) arrives when its turn comes.
    BubbleEntrance {
        #[default]
        Cut => "Instantanée",
        Fade => "Fondu",
        Pop => "Pop",
        Zoom => "Zoom",
        SlideUp => "Glisse vers le haut",
        SlideDown => "Glisse vers le bas",
        SlideLeft => "Glisse vers la gauche",
        SlideRight => "Glisse vers la droite",
        Drop => "Chute avec rebond",
    }
);

studio_choice!(
    /// Continuous motion applied to the whole bubble while its line plays.
    BubbleEmphasis {
        #[default]
        None => "Aucune",
        Shake => "Tremblement",
        Pulse => "Pulsation",
        Float => "Flottement",
        Bounce => "Sautillement",
    }
);

studio_choice!(
    /// How the text characters are revealed.
    TextReveal {
        #[default]
        Instant => "Tout d'un coup",
        Typewriter => "Machine à écrire",
        Words => "Mot par mot",
    }
);

studio_choice!(
    /// Whole-screen effect triggered when the bubble appears.
    ScreenEffect {
        #[default]
        None => "Aucun",
        Shake => "Secousse d'écran",
        Flash => "Flash blanc",
        Impact => "Impact (zoom + secousse)",
    }
);

studio_choice!(
    /// How the camera reaches a shot.
    ShotMovement {
        #[default]
        Smooth => "Mouvement fluide",
        Cut => "Coupe franche",
    }
);

studio_choice!(
    /// Transition played when a page starts.
    PageTransition {
        #[default]
        Cut => "Coupe franche",
        FadeBlack => "Fondu au noir",
        CrossFade => "Fondu enchaîné",
        SlideLeft => "Glissement latéral",
        SlideUp => "Glissement vertical",
        Zoom => "Zoom",
        Flash => "Flash blanc",
    }
);

studio_choice!(
    /// Slow "Ken Burns" drift over the whole duration of a page.
    PageMotion {
        #[default]
        None => "Aucun",
        ZoomIn => "Zoom avant lent",
        ZoomOut => "Zoom arrière lent",
        PanRight => "Travelling vers la droite",
        PanLeft => "Travelling vers la gauche",
        PanDown => "Travelling vers le bas",
        PanUp => "Travelling vers le haut",
    }
);

studio_choice!(
    /// Ready-made bubble looks. Applying one sets style, text and effects.
    BubblePreset {
        #[default]
        Classic => "Classique",
        Shout => "Cri",
        Thought => "Pensée",
        Whisper => "Chuchotement",
        Narration => "Narration",
        Radio => "Radio / écran",
        Sound => "Onomatopée",
    }
);

/// Normalized page rectangle used for custom camera framing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Region {
    pub fn from_corners(a: Point, b: Point) -> Option<Self> {
        let region = Self {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
            width: (a.x - b.x).abs(),
            height: (a.y - b.y).abs(),
        };
        region.sanitized()
    }

    pub fn sanitized(self) -> Option<Self> {
        if ![self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
        {
            return None;
        }
        let x = self.x.clamp(0.0, 1.0);
        let y = self.y.clamp(0.0, 1.0);
        let width = self.width.min(1.0 - x);
        let height = self.height.min(1.0 - y);
        (width >= 0.02 && height >= 0.02).then_some(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub fn center(self) -> Point {
        Point {
            x: self.x + self.width * 0.5,
            y: self.y + self.height * 0.5,
        }
    }

    pub fn contains(self, point: Point) -> bool {
        (self.x..=self.x + self.width).contains(&point.x)
            && (self.y..=self.y + self.height).contains(&point.y)
    }

    /// Smallest region shaped like the video frame (`aspect` = width / height
    /// in pixels) that contains `self`, kept on the page. Too large a region
    /// shrinks around its center.
    pub fn fitted(self, page_w: u32, page_h: u32, aspect: f32) -> Self {
        let (page_w, page_h) = (page_w.max(1) as f32, page_h.max(1) as f32);
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            16.0 / 9.0
        };
        let (mut width, mut height) = (self.width * page_w, self.height * page_h);
        if width < height * aspect {
            width = height * aspect;
        } else {
            height = width / aspect;
        }
        let shrink = (page_w / width).min(page_h / height).min(1.0);
        width *= shrink;
        height *= shrink;
        let center = self.center();
        let x = (center.x * page_w - width * 0.5).clamp(0.0, page_w - width);
        let y = (center.y * page_h - height * 0.5).clamp(0.0, page_h - height);
        Self {
            x: x / page_w,
            y: y / page_h,
            width: width / page_w,
            height: height / page_h,
        }
    }
}

/// A camera framing of a page. Shots play in their order: the camera moves
/// to the shot, then the bubbles it frames are read.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraShot {
    pub id: ShotId,
    /// `None` frames the whole page.
    #[serde(default)]
    pub region: Option<Region>,
    #[serde(default)]
    pub movement: ShotMovement,
    #[serde(default = "default_shot_move_ms")]
    pub move_ms: u64,
    /// Time spent on the shot before its first bubble (or alone when it
    /// frames no bubble).
    #[serde(default)]
    pub hold_ms: u64,
}

const fn default_shot_move_ms() -> u64 {
    800
}

/// Duration of a shot that frames no bubble and has no explicit hold.
pub const EMPTY_SHOT_HOLD_MS: u64 = 1_500;

impl CameraShot {
    pub fn sanitized(mut self) -> Self {
        self.region = self.region.and_then(Region::sanitized);
        self.move_ms = self.move_ms.clamp(100, 5_000);
        self.hold_ms = self.hold_ms.min(20_000);
        self
    }

    /// Area used to pick the most specific shot framing a bubble.
    fn area(&self) -> f32 {
        self.region
            .map_or(1.0, |region| region.width * region.height)
    }

    fn frames(&self, point: Point) -> bool {
        self.region.is_none_or(|region| region.contains(point))
    }
}

/// Visual extras of a bubble on top of the historical fill and text style.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BubbleLook {
    /// `None` keeps the historical thin light outline.
    pub outline_color: Option<[u8; 4]>,
    pub outline_width: f32,
    pub shadow: bool,
    pub italic: bool,
    /// `None` disables the text outline.
    pub text_outline_color: Option<[u8; 4]>,
    pub text_outline_width: f32,
}

impl Default for BubbleLook {
    fn default() -> Self {
        Self {
            outline_color: None,
            outline_width: 1.0,
            shadow: false,
            italic: false,
            text_outline_color: None,
            text_outline_width: 2.0,
        }
    }
}

impl BubbleLook {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn sanitized(mut self) -> Self {
        self.outline_width = finite_or(self.outline_width, 1.0).clamp(0.0, 12.0);
        self.text_outline_width = finite_or(self.text_outline_width, 2.0).clamp(0.5, 8.0);
        self.outline_color = self.outline_color.map(opaque);
        self.text_outline_color = self.text_outline_color.map(opaque);
        self
    }
}

/// Animation and screen effects of a bubble, and the shot that frames it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BubbleFx {
    pub entrance: BubbleEntrance,
    pub entrance_ms: u64,
    /// Hide the whole bubble before its turn and animate it entirely.
    /// Otherwise the shape stays visible to mask the original lettering and
    /// only the translated text is animated.
    pub whole_bubble: bool,
    pub text_reveal: TextReveal,
    pub emphasis: BubbleEmphasis,
    pub emphasis_strength: f32,
    pub screen_effect: ScreenEffect,
    pub screen_effect_ms: u64,
    /// Fade the text (or the whole bubble) out once its line is over.
    pub exit_after: bool,
    /// Shot that frames this bubble; `None` picks the smallest shot of the
    /// page containing it.
    pub shot: Option<ShotId>,
}

impl Default for BubbleFx {
    fn default() -> Self {
        Self {
            entrance: BubbleEntrance::Cut,
            entrance_ms: 300,
            whole_bubble: false,
            text_reveal: TextReveal::Instant,
            emphasis: BubbleEmphasis::None,
            emphasis_strength: 1.0,
            screen_effect: ScreenEffect::None,
            screen_effect_ms: 450,
            exit_after: false,
            shot: None,
        }
    }
}

impl BubbleFx {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn sanitized(mut self) -> Self {
        self.entrance_ms = self.entrance_ms.clamp(50, 5_000);
        self.emphasis_strength = finite_or(self.emphasis_strength, 1.0).clamp(0.25, 3.0);
        self.screen_effect_ms = self.screen_effect_ms.clamp(100, 5_000);
        self
    }
}

/// Per-bubble audio mix and timing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BubbleSound {
    pub voice_volume: f32,
    pub audio_delay_ms: u64,
    pub extra_hold_ms: u64,
    pub sfx_audio_id: Option<ComicAudioId>,
    pub sfx_volume: f32,
}

impl Default for BubbleSound {
    fn default() -> Self {
        Self {
            voice_volume: 1.0,
            audio_delay_ms: 0,
            extra_hold_ms: 0,
            sfx_audio_id: None,
            sfx_volume: 1.0,
        }
    }
}

impl BubbleSound {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn sanitized(mut self) -> Self {
        self.voice_volume = finite_or(self.voice_volume, 1.0).clamp(0.0, 2.0);
        self.sfx_volume = finite_or(self.sfx_volume, 1.0).clamp(0.0, 2.0);
        self.audio_delay_ms = self.audio_delay_ms.min(10_000);
        self.extra_hold_ms = self.extra_hold_ms.min(30_000);
        self
    }
}

/// Page-level staging: transition, establishing hold and slow camera drift.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageFx {
    pub transition: PageTransition,
    pub transition_ms: u64,
    pub intro_ms: u64,
    pub motion: PageMotion,
    pub motion_strength: f32,
}

impl Default for PageFx {
    fn default() -> Self {
        Self {
            transition: PageTransition::Cut,
            transition_ms: 600,
            intro_ms: 0,
            motion: PageMotion::None,
            motion_strength: 1.0,
        }
    }
}

impl PageFx {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn sanitized(mut self) -> Self {
        self.transition_ms = self.transition_ms.clamp(100, 5_000);
        self.intro_ms = self.intro_ms.min(20_000);
        self.motion_strength = finite_or(self.motion_strength, 1.0).clamp(0.25, 3.0);
        self
    }

    /// Duration the transition actually occupies on the timeline.
    pub fn effective_transition_ms(&self) -> u64 {
        if self.transition == PageTransition::Cut {
            0
        } else {
            self.transition_ms
        }
    }
}

/// Project-wide mix and look of the Comic Dub.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StudioSettings {
    pub music_audio_id: Option<ComicAudioId>,
    pub music_volume: f32,
    pub music_loop: bool,
    pub music_ducking: bool,
    pub music_fade_out_ms: u64,
    /// Letterbox and transition color.
    pub background: [u8; 3],
    /// Characters per second of the typewriter reveal on silent bubbles.
    pub typewriter_cps: f32,
}

impl Default for StudioSettings {
    fn default() -> Self {
        Self {
            music_audio_id: None,
            music_volume: 0.35,
            music_loop: true,
            music_ducking: true,
            music_fade_out_ms: 1_500,
            background: [0, 0, 0],
            typewriter_cps: 28.0,
        }
    }
}

impl StudioSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn sanitized(mut self) -> Self {
        self.music_volume = finite_or(self.music_volume, 0.35).clamp(0.0, 1.0);
        self.music_fade_out_ms = self.music_fade_out_ms.min(10_000);
        self.typewriter_cps = finite_or(self.typewriter_cps, 28.0).clamp(5.0, 120.0);
        self
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn opaque(color: [u8; 4]) -> [u8; 4] {
    [color[0], color[1], color[2], 255]
}

/// One meaningful line of a translation script.
#[derive(Debug, Clone, PartialEq)]
enum ScriptEntry {
    Page(usize),
    Text(String),
}

fn parse_script(script: &str) -> Vec<ScriptEntry> {
    script
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            if let Some(comment) = line.strip_prefix('#') {
                let comment = comment.trim();
                let number = comment
                    .get(..4)
                    .filter(|word| word.eq_ignore_ascii_case("page"))
                    .and_then(|_| {
                        comment[4..]
                            .trim_start()
                            .split(|character: char| !character.is_ascii_digit())
                            .next()
                    })
                    .and_then(|digits| digits.parse::<usize>().ok())?;
                return number.checked_sub(1).map(ScriptEntry::Page);
            }
            Some(ScriptEntry::Text(if line == "-" {
                String::new()
            } else {
                line.to_string()
            }))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VertexKeyframe {
    pub at_ms: u64,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bubble {
    pub id: BubbleId,
    pub points: Vec<Point>,
    pub text: String,
    pub color: [u8; 4],
    #[serde(default = "default_bubble_font_size")]
    pub font_size: f32,
    #[serde(default)]
    pub letter_spacing: f32,
    #[serde(default = "default_line_spacing")]
    pub line_spacing: f32,
    #[serde(default)]
    pub text_color: Option<[u8; 4]>,
    #[serde(default)]
    pub text_alignment: TextAlignment,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub strikethrough: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub audio_id: Option<ComicAudioId>,
    #[serde(default)]
    pub vertex_keyframes: Vec<VertexKeyframe>,
    #[serde(default, skip_serializing_if = "BubbleLook::is_default")]
    pub look: BubbleLook,
    #[serde(default, skip_serializing_if = "BubbleFx::is_default")]
    pub fx: BubbleFx,
    #[serde(default, skip_serializing_if = "BubbleSound::is_default")]
    pub sound: BubbleSound,
}

impl Bubble {
    /// Vertex animation is deliberately stepped: the last pose at or before
    /// `at_ms` wins, with no interpolation between poses.
    pub fn points_at(&self, at_ms: u64) -> &[Point] {
        self.vertex_keyframes
            .iter()
            .rev()
            .find(|keyframe| keyframe.at_ms <= at_ms)
            .map_or(&self.points, |keyframe| &keyframe.points)
    }

    pub fn vertex_animation_duration_ms(&self) -> u64 {
        self.vertex_keyframes
            .last()
            .map_or(0, |keyframe| keyframe.at_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    pub file_name: String,
    pub width: u32,
    pub height: u32,
    #[serde(skip)]
    pub image_path: PathBuf,
    #[serde(default)]
    pub bubbles: Vec<Bubble>,
    #[serde(default, skip_serializing_if = "PageFx::is_default")]
    pub fx: PageFx,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shots: Vec<CameraShot>,
}

impl Page {
    /// Shot index of every bubble: its chosen shot, else the smallest shot
    /// framing its center, else the shot of the previous bubble, else the
    /// first shot. `None` everywhere when the page has no shot.
    pub fn bubble_shots(&self) -> Vec<Option<usize>> {
        let mut previous = None;
        self.bubbles
            .iter()
            .map(|bubble| {
                if self.shots.is_empty() {
                    return None;
                }
                let explicit = bubble
                    .fx
                    .shot
                    .and_then(|id| self.shots.iter().position(|shot| shot.id == id));
                let center = bubble_center(&bubble.points);
                let framing = || {
                    self.shots
                        .iter()
                        .enumerate()
                        .filter(|(_, shot)| shot.frames(center))
                        .min_by(|a, b| a.1.area().total_cmp(&b.1.area()))
                        .map(|(index, _)| index)
                };
                let shot = explicit.or_else(framing).or(previous).unwrap_or(0);
                previous = Some(shot);
                Some(shot)
            })
            .collect()
    }

    pub fn shot_index(&self, id: ShotId) -> Option<usize> {
        self.shots.iter().position(|shot| shot.id == id)
    }
}

/// Center of a polygon's bounding box.
pub fn bubble_center(points: &[Point]) -> Point {
    let min_x = points.iter().map(|point| point.x).fold(1.0, f32::min);
    let max_x = points.iter().map(|point| point.x).fold(0.0, f32::max);
    let min_y = points.iter().map(|point| point.y).fold(1.0, f32::min);
    let max_y = points.iter().map(|point| point.y).fold(0.0, f32::max);
    Point {
        x: (min_x + max_x) * 0.5,
        y: (min_y + max_y) * 0.5,
    }
}

/// Bounding box of a polygon as a region (unsanitized).
pub fn bubble_bounds(points: &[Point]) -> Region {
    let min_x = points.iter().map(|point| point.x).fold(1.0, f32::min);
    let max_x = points.iter().map(|point| point.x).fold(0.0, f32::max);
    let min_y = points.iter().map(|point| point.y).fold(1.0, f32::min);
    let max_y = points.iter().map(|point| point.y).fold(0.0, f32::max);
    Region {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(0.0),
        height: (max_y - min_y).max(0.0),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComicAudio {
    pub id: ComicAudioId,
    pub file_name: String,
    #[serde(skip)]
    pub playback_path: PathBuf,
    pub sample_rate: u32,
    pub sample_count: u64,
    #[serde(skip)]
    pub waveform: WaveformData,
}

impl ComicAudio {
    pub fn duration_ms(&self) -> u64 {
        self.sample_count
            .saturating_mul(1_000)
            .checked_div(u64::from(self.sample_rate.max(1)))
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComicDubsProject {
    #[serde(default)]
    pages: Vec<Page>,
    #[serde(default)]
    audios: Vec<ComicAudio>,
    #[serde(default)]
    active_page: Option<PageId>,
    #[serde(default = "default_bubble_gap_ms")]
    bubble_gap_ms: u64,
    #[serde(default = "default_page_gap_ms")]
    page_gap_ms: u64,
    #[serde(default)]
    font_family: Option<String>,
    #[serde(default = "default_bubble_font_size")]
    default_font_size: f32,
    #[serde(default = "first_id")]
    next_id: u64,
    #[serde(default, skip_serializing_if = "StudioSettings::is_default")]
    studio: StudioSettings,
}

const fn first_id() -> u64 {
    1
}

const fn default_bubble_gap_ms() -> u64 {
    250
}

const fn default_page_gap_ms() -> u64 {
    250
}

const fn default_bubble_font_size() -> f32 {
    24.0
}

const fn default_line_spacing() -> f32 {
    1.18
}

impl Default for ComicDubsProject {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            audios: Vec::new(),
            active_page: None,
            bubble_gap_ms: default_bubble_gap_ms(),
            page_gap_ms: default_page_gap_ms(),
            font_family: None,
            default_font_size: default_bubble_font_size(),
            next_id: first_id(),
            studio: StudioSettings::default(),
        }
    }
}

impl ComicDubsProject {
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn audios(&self) -> &[ComicAudio] {
        &self.audios
    }

    pub fn active_page_id(&self) -> Option<PageId> {
        self.active_page
    }

    pub fn active_page(&self) -> Option<&Page> {
        let id = self.active_page?;
        self.pages.iter().find(|page| page.id == id)
    }

    pub fn page(&self, id: PageId) -> Option<&Page> {
        self.pages.iter().find(|page| page.id == id)
    }

    pub fn audio(&self, id: ComicAudioId) -> Option<&ComicAudio> {
        self.audios.iter().find(|audio| audio.id == id)
    }

    pub fn bubble(&self, id: BubbleId) -> Option<&Bubble> {
        self.pages
            .iter()
            .flat_map(|page| &page.bubbles)
            .find(|bubble| bubble.id == id)
    }

    pub fn bubble_gap_ms(&self) -> u64 {
        self.bubble_gap_ms
    }

    pub fn page_gap_ms(&self) -> u64 {
        self.page_gap_ms
    }

    pub fn font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }

    pub fn default_font_size(&self) -> f32 {
        self.default_font_size
    }

    pub fn set_settings(
        &mut self,
        font_family: Option<String>,
        bubble_gap_ms: u64,
        page_gap_ms: u64,
        default_font_size: f32,
    ) -> bool {
        if !default_font_size.is_finite() {
            return false;
        }
        let font_family = font_family.and_then(|font| {
            let font = clean_name(&font, "");
            (!font.is_empty()).then_some(font)
        });
        let settings = (
            font_family,
            bubble_gap_ms.min(60_000),
            page_gap_ms.min(60_000),
            default_font_size.clamp(6.0, 72.0),
        );
        if (
            &self.font_family,
            self.bubble_gap_ms,
            self.page_gap_ms,
            self.default_font_size,
        ) == (&settings.0, settings.1, settings.2, settings.3)
        {
            return false;
        }
        (
            self.font_family,
            self.bubble_gap_ms,
            self.page_gap_ms,
            self.default_font_size,
        ) = settings;
        true
    }

    pub fn set_gaps(&mut self, bubble_gap_ms: u64, page_gap_ms: u64) -> bool {
        let gaps = (bubble_gap_ms.min(60_000), page_gap_ms.min(60_000));
        if (self.bubble_gap_ms, self.page_gap_ms) == gaps {
            return false;
        }
        (self.bubble_gap_ms, self.page_gap_ms) = gaps;
        true
    }

    pub fn add_page(
        &mut self,
        file_name: String,
        image_path: PathBuf,
        width: u32,
        height: u32,
    ) -> PageId {
        let id = self.allocate_id();
        self.pages.push(Page {
            id,
            file_name: clean_name(&file_name, "page.png"),
            width: width.max(1),
            height: height.max(1),
            image_path,
            bubbles: Vec::new(),
            fx: PageFx::default(),
            shots: Vec::new(),
        });
        self.active_page = Some(id);
        id
    }

    pub fn bind_page(&mut self, id: PageId, image_path: PathBuf) -> bool {
        let Some(page) = self.pages.iter_mut().find(|page| page.id == id) else {
            return false;
        };
        page.image_path = image_path;
        true
    }

    pub fn select_page(&mut self, id: PageId) -> bool {
        if self.active_page == Some(id) || self.page(id).is_none() {
            return false;
        }
        self.active_page = Some(id);
        true
    }

    pub fn remove_page(&mut self, id: PageId) -> bool {
        let Some(index) = self.pages.iter().position(|page| page.id == id) else {
            return false;
        };
        self.pages.remove(index);
        if self.active_page == Some(id) {
            self.active_page = self
                .pages
                .get(index.min(self.pages.len().saturating_sub(1)))
                .map(|page| page.id);
        }
        true
    }

    pub fn move_page(&mut self, id: PageId, delta: isize) -> bool {
        let Some(from) = self.pages.iter().position(|page| page.id == id) else {
            return false;
        };
        let to = from
            .saturating_add_signed(delta)
            .min(self.pages.len().saturating_sub(1));
        if from == to {
            return false;
        }
        let page = self.pages.remove(from);
        self.pages.insert(to, page);
        true
    }

    pub fn add_audio(
        &mut self,
        file_name: String,
        playback_path: PathBuf,
        recorded: RecordedAudio,
    ) -> ComicAudioId {
        let id = self.allocate_id();
        self.audios.push(ComicAudio {
            id,
            file_name: clean_name(&file_name, &recorded.file_name),
            playback_path,
            sample_rate: recorded.sample_rate,
            sample_count: recorded.sample_count,
            waveform: recorded.waveform,
        });
        id
    }

    pub fn bind_audio(
        &mut self,
        id: ComicAudioId,
        playback_path: PathBuf,
        recorded: RecordedAudio,
    ) -> bool {
        let Some(audio) = self.audios.iter_mut().find(|audio| audio.id == id) else {
            return false;
        };
        audio.playback_path = playback_path;
        audio.sample_rate = recorded.sample_rate;
        audio.sample_count = recorded.sample_count;
        audio.waveform = recorded.waveform;
        true
    }

    pub fn remove_audio(&mut self, id: ComicAudioId) -> bool {
        let before = self.audios.len();
        self.audios.retain(|audio| audio.id != id);
        if self.audios.len() == before {
            return false;
        }
        for page in &mut self.pages {
            for bubble in &mut page.bubbles {
                if bubble.audio_id == Some(id) {
                    bubble.audio_id = None;
                }
                if bubble.sound.sfx_audio_id == Some(id) {
                    bubble.sound.sfx_audio_id = None;
                }
            }
        }
        if self.studio.music_audio_id == Some(id) {
            self.studio.music_audio_id = None;
        }
        true
    }

    pub fn add_bubble(&mut self, page_id: PageId, points: Vec<Point>) -> Option<BubbleId> {
        if !valid_polygon(&points) {
            return None;
        }
        let id = self.allocate_id();
        let font_size = self.default_font_size;
        let page = self.pages.iter_mut().find(|page| page.id == page_id)?;
        page.bubbles.push(Bubble {
            id,
            points,
            text: String::new(),
            color: [255, 255, 255, 255],
            font_size,
            letter_spacing: 0.0,
            line_spacing: default_line_spacing(),
            text_color: None,
            text_alignment: TextAlignment::Center,
            bold: false,
            strikethrough: false,
            underline: false,
            audio_id: None,
            vertex_keyframes: Vec::new(),
            look: BubbleLook::default(),
            fx: BubbleFx::default(),
            sound: BubbleSound::default(),
        });
        Some(id)
    }

    pub fn set_bubble_text(&mut self, id: BubbleId, text: String) -> bool {
        let text = clean_name(&text, "");
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.text == text {
            return false;
        }
        bubble.text = text;
        true
    }

    pub fn set_bubble_color(&mut self, id: BubbleId, color: [u8; 4]) -> bool {
        let color = [
            color[0],
            color[1],
            color[2],
            if color[3] == 0 { 0 } else { 255 },
        ];
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.color == color {
            return false;
        }
        bubble.color = color;
        true
    }

    pub fn set_bubble_font_size(&mut self, id: BubbleId, font_size: f32) -> bool {
        if !font_size.is_finite() {
            return false;
        }
        let font_size = font_size.clamp(6.0, 72.0);
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.font_size == font_size {
            return false;
        }
        bubble.font_size = font_size;
        true
    }

    pub fn set_bubble_letter_spacing(&mut self, id: BubbleId, spacing: f32) -> bool {
        if !spacing.is_finite() {
            return false;
        }
        let spacing = spacing.clamp(0.0, 12.0);
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.letter_spacing == spacing {
            return false;
        }
        bubble.letter_spacing = spacing;
        true
    }

    pub fn set_bubble_line_spacing(&mut self, id: BubbleId, spacing: f32) -> bool {
        if !spacing.is_finite() {
            return false;
        }
        let spacing = spacing.clamp(0.8, 2.0);
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.line_spacing == spacing {
            return false;
        }
        bubble.line_spacing = spacing;
        true
    }

    pub fn set_bubble_text_color(&mut self, id: BubbleId, color: [u8; 4]) -> bool {
        let color = Some([color[0], color[1], color[2], 255]);
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.text_color == color {
            return false;
        }
        bubble.text_color = color;
        true
    }

    pub fn set_bubble_text_alignment(&mut self, id: BubbleId, alignment: TextAlignment) -> bool {
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.text_alignment == alignment {
            return false;
        }
        bubble.text_alignment = alignment;
        true
    }

    pub fn set_bubble_text_style(
        &mut self,
        id: BubbleId,
        bold: bool,
        strikethrough: bool,
        underline: bool,
    ) -> bool {
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if (bubble.bold, bubble.strikethrough, bubble.underline) == (bold, strikethrough, underline)
        {
            return false;
        }
        bubble.bold = bold;
        bubble.strikethrough = strikethrough;
        bubble.underline = underline;
        true
    }

    pub fn set_bubble_points(&mut self, id: BubbleId, points: Vec<Point>) -> bool {
        if !valid_polygon(&points) {
            return false;
        }
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.points == points {
            return false;
        }
        bubble.points = points;
        true
    }

    pub fn set_bubble_vertex_keyframe(
        &mut self,
        id: BubbleId,
        at_ms: u64,
        points: Vec<Point>,
    ) -> bool {
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if points.len() != bubble.points.len() || !valid_polygon(&points) {
            return false;
        }
        let at_ms = at_ms.min(86_400_000);
        match bubble
            .vertex_keyframes
            .binary_search_by_key(&at_ms, |keyframe| keyframe.at_ms)
        {
            Ok(index) if bubble.vertex_keyframes[index].points == points => false,
            Ok(index) => {
                bubble.vertex_keyframes[index].points = points;
                true
            }
            Err(index) => {
                bubble
                    .vertex_keyframes
                    .insert(index, VertexKeyframe { at_ms, points });
                true
            }
        }
    }

    pub fn remove_bubble_vertex_keyframe(&mut self, id: BubbleId, at_ms: u64) -> bool {
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        let before = bubble.vertex_keyframes.len();
        bubble
            .vertex_keyframes
            .retain(|keyframe| keyframe.at_ms != at_ms);
        bubble.vertex_keyframes.len() != before
    }

    pub fn assign_audio(&mut self, bubble_id: BubbleId, audio_id: Option<ComicAudioId>) -> bool {
        if audio_id.is_some_and(|id| self.audio(id).is_none()) {
            return false;
        }
        let Some(bubble) = self.bubble_mut(bubble_id) else {
            return false;
        };
        if bubble.audio_id == audio_id {
            return false;
        }
        bubble.audio_id = audio_id;
        true
    }

    pub fn remove_bubble(&mut self, id: BubbleId) -> bool {
        for page in &mut self.pages {
            let before = page.bubbles.len();
            page.bubbles.retain(|bubble| bubble.id != id);
            if page.bubbles.len() != before {
                return true;
            }
        }
        false
    }

    pub fn move_bubble(&mut self, id: BubbleId, delta: isize) -> bool {
        let Some(page) = self
            .pages
            .iter_mut()
            .find(|page| page.bubbles.iter().any(|bubble| bubble.id == id))
        else {
            return false;
        };
        let from = page
            .bubbles
            .iter()
            .position(|bubble| bubble.id == id)
            .unwrap();
        let to = from
            .saturating_add_signed(delta)
            .min(page.bubbles.len().saturating_sub(1));
        if from == to {
            return false;
        }
        let bubble = page.bubbles.remove(from);
        page.bubbles.insert(to, bubble);
        true
    }

    pub fn studio(&self) -> &StudioSettings {
        &self.studio
    }

    pub fn set_studio(&mut self, settings: StudioSettings) -> bool {
        let settings = settings.sanitized();
        if settings
            .music_audio_id
            .is_some_and(|id| self.audio(id).is_none())
            || self.studio == settings
        {
            return false;
        }
        self.studio = settings;
        true
    }

    pub fn set_page_fx(&mut self, id: PageId, fx: PageFx) -> bool {
        let fx = fx.sanitized();
        let Some(page) = self.pages.iter_mut().find(|page| page.id == id) else {
            return false;
        };
        if page.fx == fx {
            return false;
        }
        page.fx = fx;
        true
    }

    pub fn set_bubble_fx(&mut self, id: BubbleId, fx: BubbleFx) -> bool {
        let fx = fx.sanitized();
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.fx == fx {
            return false;
        }
        bubble.fx = fx;
        true
    }

    pub fn set_bubble_look(&mut self, id: BubbleId, look: BubbleLook) -> bool {
        let look = look.sanitized();
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.look == look {
            return false;
        }
        bubble.look = look;
        true
    }

    pub fn set_bubble_sound(&mut self, id: BubbleId, sound: BubbleSound) -> bool {
        let sound = sound.sanitized();
        if sound
            .sfx_audio_id
            .is_some_and(|audio| self.audio(audio).is_none())
        {
            return false;
        }
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        if bubble.sound == sound {
            return false;
        }
        bubble.sound = sound;
        true
    }

    pub fn apply_bubble_preset(&mut self, id: BubbleId, preset: BubblePreset) -> bool {
        let default_font_size = self.default_font_size;
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        let before = bubble.clone();
        apply_preset(bubble, preset, default_font_size);
        *bubble != before
    }

    /// Copies the look, text style and effects of `source`, keeping the
    /// target's text, shape, audio and shot.
    pub fn copy_bubble_style(&mut self, target: BubbleId, source: &Bubble) -> bool {
        let Some(bubble) = self.bubble_mut(target) else {
            return false;
        };
        let before = bubble.clone();
        copy_style(bubble, source);
        *bubble != before
    }

    pub fn apply_bubble_style_to_page(&mut self, source_id: BubbleId) -> bool {
        let Some(source) = self.bubble(source_id).cloned() else {
            return false;
        };
        let Some(page) = self
            .pages
            .iter_mut()
            .find(|page| page.bubbles.iter().any(|bubble| bubble.id == source_id))
        else {
            return false;
        };
        let mut changed = false;
        for bubble in page
            .bubbles
            .iter_mut()
            .filter(|bubble| bubble.id != source_id)
        {
            let before = bubble.clone();
            copy_style(bubble, &source);
            changed |= *bubble != before;
        }
        changed
    }

    pub fn shot(&self, id: ShotId) -> Option<&CameraShot> {
        self.pages
            .iter()
            .flat_map(|page| &page.shots)
            .find(|shot| shot.id == id)
    }

    pub fn page_of_shot(&self, id: ShotId) -> Option<PageId> {
        self.pages
            .iter()
            .find(|page| page.shot_index(id).is_some())
            .map(|page| page.id)
    }

    /// Adds a shot at the end of the page's shot order.
    pub fn add_shot(&mut self, page_id: PageId, region: Option<Region>) -> Option<ShotId> {
        let region = match region {
            Some(region) => Some(region.sanitized()?),
            None => None,
        };
        self.page(page_id)?;
        let id = self.allocate_id();
        let page = self.pages.iter_mut().find(|page| page.id == page_id)?;
        page.shots.push(CameraShot {
            id,
            region,
            movement: ShotMovement::Smooth,
            move_ms: default_shot_move_ms(),
            hold_ms: 0,
        });
        Some(id)
    }

    /// Adds a shot framing a bubble with some margin, placed right after the
    /// shot that currently frames it so the reading order stays natural.
    pub fn add_shot_around_bubble(&mut self, bubble_id: BubbleId, aspect: f32) -> Option<ShotId> {
        let page_id = self.page_of_bubble(bubble_id)?;
        let page = self.page(page_id)?;
        let index = page
            .bubbles
            .iter()
            .position(|bubble| bubble.id == bubble_id)?;
        let bounds = bubble_bounds(&page.bubbles[index].points);
        let margin_x = bounds.width * 0.25 + 0.03;
        let margin_y = bounds.height * 0.25 + 0.03;
        let region = Region {
            x: bounds.x - margin_x,
            y: bounds.y - margin_y,
            width: bounds.width + margin_x * 2.0,
            height: bounds.height + margin_y * 2.0,
        }
        .fitted(page.width, page.height, aspect)
        .sanitized()?;
        let position = page.bubble_shots()[index].map_or(page.shots.len(), |shot| shot + 1);
        let id = self.add_shot(page_id, Some(region))?;
        let page = self.pages.iter_mut().find(|page| page.id == page_id)?;
        let shot = page.shots.pop()?;
        let position = position.min(page.shots.len());
        page.shots.insert(position, shot);
        Some(id)
    }

    /// Updates a shot's framing and timing; its id selects it.
    pub fn set_shot(&mut self, shot: CameraShot) -> bool {
        let shot = shot.sanitized();
        let Some(current) = self
            .pages
            .iter_mut()
            .flat_map(|page| &mut page.shots)
            .find(|candidate| candidate.id == shot.id)
        else {
            return false;
        };
        if *current == shot {
            return false;
        }
        *current = shot;
        true
    }

    pub fn remove_shot(&mut self, id: ShotId) -> bool {
        for page in &mut self.pages {
            let before = page.shots.len();
            page.shots.retain(|shot| shot.id != id);
            if page.shots.len() != before {
                for bubble in &mut page.bubbles {
                    if bubble.fx.shot == Some(id) {
                        bubble.fx.shot = None;
                    }
                }
                return true;
            }
        }
        false
    }

    pub fn move_shot(&mut self, id: ShotId, delta: isize) -> bool {
        let Some(page) = self
            .pages
            .iter_mut()
            .find(|page| page.shot_index(id).is_some())
        else {
            return false;
        };
        let from = page.shot_index(id).unwrap();
        let to = from
            .saturating_add_signed(delta)
            .min(page.shots.len().saturating_sub(1));
        if from == to {
            return false;
        }
        let shot = page.shots.remove(from);
        page.shots.insert(to, shot);
        true
    }

    pub fn page_of_bubble(&self, id: BubbleId) -> Option<PageId> {
        self.pages
            .iter()
            .find(|page| page.bubbles.iter().any(|bubble| bubble.id == id))
            .map(|page| page.id)
    }

    /// Inserts a copy of `source` on `page_id`, right after `after` when it
    /// belongs to that page, otherwise at the end of the reading order.
    pub fn paste_bubble(
        &mut self,
        page_id: PageId,
        source: &Bubble,
        offset: Point,
        after: Option<BubbleId>,
    ) -> Option<BubbleId> {
        if self.page(page_id).is_none() || !valid_polygon(&source.points) {
            return None;
        }
        let mut bubble = source.clone();
        bubble.audio_id = source.audio_id.filter(|id| self.audio(*id).is_some());
        bubble.sound.sfx_audio_id = source
            .sound
            .sfx_audio_id
            .filter(|id| self.audio(*id).is_some());
        bubble.id = self.allocate_id();
        translate_poses(&mut bubble, offset);
        let id = bubble.id;
        let page = self.pages.iter_mut().find(|page| page.id == page_id)?;
        bubble.fx.shot = bubble
            .fx
            .shot
            .filter(|shot| page.shot_index(*shot).is_some());
        let index = after
            .and_then(|after| page.bubbles.iter().position(|bubble| bubble.id == after))
            .map_or(page.bubbles.len(), |index| index + 1);
        page.bubbles.insert(index, bubble);
        Some(id)
    }

    pub fn duplicate_bubble(&mut self, id: BubbleId) -> Option<BubbleId> {
        let page_id = self.page_of_bubble(id)?;
        let source = self.bubble(id)?.clone();
        self.paste_bubble(page_id, &source, Point { x: 0.02, y: 0.02 }, Some(id))
    }

    /// Moves every pose of a bubble, clamped so the shape stays on the page.
    pub fn translate_bubble(&mut self, id: BubbleId, delta: Point) -> bool {
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        let before = bubble.clone();
        translate_poses(bubble, delta);
        *bubble != before
    }

    /// Adds a vertex after `after`. Animated poses receive the midpoint of
    /// their matching edge so every pose keeps the same vertex count.
    pub fn insert_bubble_vertex(&mut self, id: BubbleId, after: usize, point: Point) -> bool {
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        let count = bubble.points.len();
        if after >= count || count >= 128 {
            return false;
        }
        let mut points = bubble.points.clone();
        points.insert(after + 1, point);
        if !valid_polygon(&points) {
            return false;
        }
        bubble.points = points;
        for keyframe in &mut bubble.vertex_keyframes {
            let a = keyframe.points[after];
            let b = keyframe.points[(after + 1) % count];
            keyframe.points.insert(
                after + 1,
                Point {
                    x: (a.x + b.x) * 0.5,
                    y: (a.y + b.y) * 0.5,
                },
            );
        }
        true
    }

    pub fn remove_bubble_vertex(&mut self, id: BubbleId, index: usize) -> bool {
        self.reshape_bubble(id, |points| {
            if points.len() <= 3 || index >= points.len() {
                return points.to_vec();
            }
            let mut points = points.to_vec();
            points.remove(index);
            points
        })
    }

    pub fn add_bubble_tail(&mut self, id: BubbleId) -> bool {
        self.reshape_bubble(id, crate::comic_dubs_shapes::with_tail)
    }

    pub fn smooth_bubble(&mut self, id: BubbleId) -> bool {
        self.reshape_bubble(id, crate::comic_dubs_shapes::smoothed)
    }

    /// Applies a deterministic shape operation to the rest pose and every
    /// animated pose, atomically.
    fn reshape_bubble(&mut self, id: BubbleId, shape: impl Fn(&[Point]) -> Vec<Point>) -> bool {
        let Some(bubble) = self.bubble_mut(id) else {
            return false;
        };
        let points = shape(&bubble.points);
        let poses = bubble
            .vertex_keyframes
            .iter()
            .map(|keyframe| shape(&keyframe.points))
            .collect::<Vec<_>>();
        if points == bubble.points
            || !valid_polygon(&points)
            || poses
                .iter()
                .any(|pose| pose.len() != points.len() || !valid_polygon(pose))
        {
            return false;
        }
        bubble.points = points;
        for (keyframe, pose) in bubble.vertex_keyframes.iter_mut().zip(poses) {
            keyframe.points = pose;
        }
        true
    }

    /// Plain-text translation script: one line per bubble in reading order,
    /// `# Page N` markers and `-` for bubbles without text.
    pub fn script_text(&self) -> String {
        let mut script = String::from(
            "# Script Comic Dubs : une ligne par bulle, « - » pour une bulle sans texte.\n",
        );
        for (index, page) in self.pages.iter().enumerate() {
            script.push_str(&format!("\n# Page {} — {}\n", index + 1, page.file_name));
            for bubble in &page.bubbles {
                let text = bubble.text.trim();
                script.push_str(if text.is_empty() { "-" } else { text });
                script.push('\n');
            }
        }
        script
    }

    /// Fills bubble texts from a script written by [`Self::script_text`] or a
    /// plain list of lines. Returns `(filled bubbles, total bubbles)`.
    pub fn apply_script(&mut self, script: &str) -> (usize, usize) {
        let order = self
            .pages
            .iter()
            .enumerate()
            .flat_map(|(page, content)| {
                (0..content.bubbles.len()).map(move |bubble| (page, bubble))
            })
            .collect::<Vec<_>>();
        let mut cursor = 0;
        let mut page_bound = None;
        let mut applied = 0;
        for entry in parse_script(script) {
            match entry {
                ScriptEntry::Page(page) => {
                    page_bound = Some(page);
                    cursor = order
                        .iter()
                        .position(|(candidate, _)| *candidate == page)
                        .unwrap_or(order.len());
                }
                ScriptEntry::Text(text) => {
                    let Some(&(page, bubble)) = order.get(cursor) else {
                        continue;
                    };
                    if page_bound.is_some_and(|bound| bound != page) {
                        continue;
                    }
                    self.pages[page].bubbles[bubble].text = clean_name(&text, "");
                    applied += 1;
                    cursor += 1;
                }
            }
        }
        (applied, order.len())
    }

    pub(crate) fn validate(&mut self) -> Result<(), String> {
        let mut ids = HashSet::new();
        let audio_ids = self
            .audios
            .iter()
            .map(|audio| audio.id)
            .collect::<HashSet<_>>();
        for page in &mut self.pages {
            if page.width == 0
                || page.height == 0
                || page.file_name.trim().is_empty()
                || !ids.insert(page.id)
            {
                return Err("invalid Comic Dubs page".into());
            }
            page.fx = page.fx.sanitized();
            for shot in &mut page.shots {
                *shot = shot.sanitized();
                if !ids.insert(shot.id) {
                    return Err("invalid Comic Dubs shot".into());
                }
            }
            let shot_ids = page
                .shots
                .iter()
                .map(|shot| shot.id)
                .collect::<HashSet<_>>();
            for bubble in &mut page.bubbles {
                if bubble.fx.shot.is_some_and(|id| !shot_ids.contains(&id)) {
                    bubble.fx.shot = None;
                }
                bubble.look = bubble.look.sanitized();
                bubble.fx = bubble.fx.sanitized();
                bubble.sound = bubble.sound.sanitized();
                if bubble
                    .sound
                    .sfx_audio_id
                    .is_some_and(|id| !audio_ids.contains(&id))
                {
                    bubble.sound.sfx_audio_id = None;
                }
                bubble.color[3] = if bubble.color[3] == 0 { 0 } else { 255 };
                if let Some(color) = &mut bubble.text_color {
                    color[3] = 255;
                }
                bubble
                    .vertex_keyframes
                    .sort_by_key(|keyframe| keyframe.at_ms);
                if !ids.insert(bubble.id)
                    || !valid_polygon(&bubble.points)
                    || !bubble.font_size.is_finite()
                    || !(6.0..=72.0).contains(&bubble.font_size)
                    || !bubble.letter_spacing.is_finite()
                    || !(0.0..=12.0).contains(&bubble.letter_spacing)
                    || !bubble.line_spacing.is_finite()
                    || !(0.8..=2.0).contains(&bubble.line_spacing)
                    || bubble.audio_id.is_some_and(|id| !audio_ids.contains(&id))
                    || bubble
                        .vertex_keyframes
                        .windows(2)
                        .any(|pair| pair[0].at_ms == pair[1].at_ms)
                    || bubble.vertex_keyframes.iter().any(|keyframe| {
                        keyframe.at_ms > 86_400_000
                            || keyframe.points.len() != bubble.points.len()
                            || !valid_polygon(&keyframe.points)
                    })
                {
                    return Err("invalid Comic Dubs bubble".into());
                }
            }
        }
        for audio in &self.audios {
            if audio.sample_rate == 0
                || audio.sample_count == 0
                || audio.file_name.trim().is_empty()
                || !ids.insert(audio.id)
            {
                return Err("invalid Comic Dubs audio".into());
            }
        }
        if self
            .active_page
            .and_then(|id| self.pages.iter().find(|page| page.id == id))
            .is_none()
        {
            self.active_page = self.pages.first().map(|page| page.id);
        }
        self.bubble_gap_ms = self.bubble_gap_ms.min(60_000);
        self.page_gap_ms = self.page_gap_ms.min(60_000);
        self.font_family = self.font_family.take().and_then(|font| {
            let font = clean_name(&font, "");
            (!font.is_empty()).then_some(font)
        });
        if !self.default_font_size.is_finite() {
            self.default_font_size = default_bubble_font_size();
        }
        self.default_font_size = self.default_font_size.clamp(6.0, 72.0);
        self.studio = self.studio.sanitized();
        if self
            .studio
            .music_audio_id
            .is_some_and(|id| !audio_ids.contains(&id))
        {
            self.studio.music_audio_id = None;
        }
        self.next_id = self
            .next_id
            .max(ids.into_iter().max().unwrap_or(0).saturating_add(1));
        Ok(())
    }

    fn bubble_mut(&mut self, id: BubbleId) -> Option<&mut Bubble> {
        self.pages
            .iter_mut()
            .flat_map(|page| &mut page.bubbles)
            .find(|bubble| bubble.id == id)
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id.saturating_add(1);
        id
    }
}

fn translate_poses(bubble: &mut Bubble, delta: Point) {
    let all = || {
        bubble.points.iter().chain(
            bubble
                .vertex_keyframes
                .iter()
                .flat_map(|keyframe| &keyframe.points),
        )
    };
    let min_x = all().map(|point| point.x).fold(1.0, f32::min);
    let max_x = all().map(|point| point.x).fold(0.0, f32::max);
    let min_y = all().map(|point| point.y).fold(1.0, f32::min);
    let max_y = all().map(|point| point.y).fold(0.0, f32::max);
    let dx = finite_or(delta.x, 0.0).clamp(-min_x, (1.0 - max_x).max(-min_x));
    let dy = finite_or(delta.y, 0.0).clamp(-min_y, (1.0 - max_y).max(-min_y));
    for point in bubble.points.iter_mut().chain(
        bubble
            .vertex_keyframes
            .iter_mut()
            .flat_map(|keyframe| &mut keyframe.points),
    ) {
        point.x = (point.x + dx).clamp(0.0, 1.0);
        point.y = (point.y + dy).clamp(0.0, 1.0);
    }
}

fn copy_style(target: &mut Bubble, source: &Bubble) {
    target.color = source.color;
    target.font_size = source.font_size;
    target.letter_spacing = source.letter_spacing;
    target.line_spacing = source.line_spacing;
    target.text_color = source.text_color;
    target.text_alignment = source.text_alignment;
    target.bold = source.bold;
    target.strikethrough = source.strikethrough;
    target.underline = source.underline;
    target.look = source.look;
    target.fx = BubbleFx {
        shot: target.fx.shot,
        ..source.fx
    };
}

fn apply_preset(bubble: &mut Bubble, preset: BubblePreset, default_font_size: f32) {
    const INK: Option<[u8; 4]> = Some([20, 20, 24, 255]);
    bubble.bold = false;
    bubble.strikethrough = false;
    bubble.underline = false;
    bubble.text_alignment = TextAlignment::Center;
    bubble.font_size = default_font_size;
    bubble.color = [255, 255, 255, 255];
    bubble.text_color = None;
    bubble.look = BubbleLook {
        outline_color: INK,
        outline_width: 2.0,
        ..BubbleLook::default()
    };
    bubble.fx = BubbleFx {
        shot: bubble.fx.shot,
        entrance: BubbleEntrance::Fade,
        entrance_ms: 220,
        ..BubbleFx::default()
    };
    match preset {
        BubblePreset::Classic => {}
        BubblePreset::Shout => {
            bubble.bold = true;
            bubble.font_size = default_font_size * 1.2;
            bubble.look.outline_width = 3.0;
            bubble.fx.entrance = BubbleEntrance::Pop;
            bubble.fx.entrance_ms = 280;
            bubble.fx.emphasis = BubbleEmphasis::Shake;
            bubble.fx.screen_effect = ScreenEffect::Shake;
        }
        BubblePreset::Thought => {
            bubble.look.italic = true;
            bubble.look.outline_color = Some([90, 90, 100, 255]);
            bubble.look.outline_width = 1.5;
            bubble.fx.entrance_ms = 400;
            bubble.fx.emphasis = BubbleEmphasis::Float;
            bubble.fx.emphasis_strength = 0.8;
        }
        BubblePreset::Whisper => {
            bubble.color = [245, 245, 250, 255];
            bubble.text_color = Some([80, 80, 90, 255]);
            bubble.font_size = default_font_size * 0.9;
            bubble.look.italic = true;
            bubble.look.outline_color = Some([150, 150, 160, 255]);
            bubble.look.outline_width = 1.0;
            bubble.fx.entrance_ms = 500;
            bubble.fx.text_reveal = TextReveal::Typewriter;
        }
        BubblePreset::Narration => {
            bubble.color = [255, 238, 170, 255];
            bubble.text_color = Some([30, 26, 20, 255]);
            bubble.text_alignment = TextAlignment::Left;
            bubble.look.shadow = true;
            bubble.fx.entrance_ms = 300;
            bubble.fx.text_reveal = TextReveal::Typewriter;
        }
        BubblePreset::Radio => {
            bubble.color = [214, 236, 255, 255];
            bubble.text_color = Some([16, 40, 90, 255]);
            bubble.look.outline_color = Some([40, 100, 200, 255]);
            bubble.fx.entrance = BubbleEntrance::Zoom;
            bubble.fx.entrance_ms = 250;
            bubble.fx.text_reveal = TextReveal::Typewriter;
            bubble.fx.emphasis = BubbleEmphasis::Pulse;
            bubble.fx.emphasis_strength = 0.5;
        }
        BubblePreset::Sound => {
            bubble.color = [255, 255, 255, 0];
            bubble.text_color = Some([255, 214, 0, 255]);
            bubble.bold = true;
            bubble.font_size = 56.0;
            bubble.look.outline_width = 0.0;
            bubble.look.text_outline_color = INK;
            bubble.look.text_outline_width = 3.5;
            bubble.fx.whole_bubble = true;
            bubble.fx.entrance = BubbleEntrance::Pop;
            bubble.fx.entrance_ms = 250;
            bubble.fx.emphasis = BubbleEmphasis::Pulse;
            bubble.fx.screen_effect = ScreenEffect::Impact;
            bubble.fx.screen_effect_ms = 500;
        }
    }
    bubble.font_size = bubble.font_size.clamp(6.0, 72.0);
}

fn clean_name(value: &str, fallback: &str) -> String {
    let value: String = value
        .chars()
        .filter(|character| !character.is_control())
        .take(500)
        .collect();
    let value = value.trim();
    if value.is_empty() {
        fallback.into()
    } else {
        value.into()
    }
}

fn valid_polygon(points: &[Point]) -> bool {
    points.len() >= 3
        && points.len() <= 128
        && points.iter().all(|point| {
            point.x.is_finite()
                && point.y.is_finite()
                && (0.0..=1.0).contains(&point.x)
                && (0.0..=1.0).contains(&point.y)
        })
        && polygon_area(points) > 0.000_01
}

fn polygon_area(points: &[Point]) -> f32 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f32>()
        .abs()
        * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> Vec<Point> {
        vec![
            Point { x: 0.1, y: 0.1 },
            Point { x: 0.9, y: 0.1 },
            Point { x: 0.5, y: 0.9 },
        ]
    }

    #[test]
    fn pages_and_bubbles_follow_explicit_reading_order() {
        let mut project = ComicDubsProject::default();
        let first = project.add_page("1.jpg".into(), "1.png".into(), 100, 200);
        let second = project.add_page("2.jpg".into(), "2.png".into(), 100, 200);
        assert!(project.move_page(second, -1));
        assert_eq!(project.pages()[0].id, second);

        let a = project.add_bubble(first, triangle()).unwrap();
        let b = project.add_bubble(first, triangle()).unwrap();
        assert!(project.move_bubble(b, -1));
        assert_eq!(project.page(first).unwrap().bubbles[0].id, b);
        assert_ne!(a, b);
    }

    #[test]
    fn removing_audio_unassigns_every_bubble() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        let audio = project.add_audio(
            "line.wav".into(),
            "line.flac".into(),
            RecordedAudio {
                file_name: "line.flac".into(),
                sample_rate: 48_000,
                channels: 1,
                sample_count: 48_000,
                checksum: "a".repeat(40),
                waveform: WaveformData::default(),
            },
        );
        assert!(project.assign_audio(bubble, Some(audio)));
        assert!(project.remove_audio(audio));
        assert_eq!(project.bubble(bubble).unwrap().audio_id, None);
    }

    #[test]
    fn bubble_style_is_per_bubble_and_text_stays_opaque() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        assert!(project.set_bubble_font_size(bubble, 36.0));
        assert!(project.set_bubble_color(bubble, [10, 20, 30, 1]));
        assert!(project.set_bubble_letter_spacing(bubble, 2.5));
        assert!(project.set_bubble_line_spacing(bubble, 1.4));
        assert!(project.set_bubble_text_color(bubble, [40, 50, 60, 1]));
        assert!(project.set_bubble_text_alignment(bubble, TextAlignment::Left));
        assert!(project.set_bubble_text_style(bubble, true, true, true));
        assert_eq!(project.bubble(bubble).unwrap().font_size, 36.0);
        assert_eq!(project.bubble(bubble).unwrap().color, [10, 20, 30, 255]);
        assert!(project.set_bubble_color(bubble, [10, 20, 30, 0]));
        assert_eq!(project.bubble(bubble).unwrap().color, [10, 20, 30, 0]);
        assert_eq!(project.bubble(bubble).unwrap().letter_spacing, 2.5);
        assert_eq!(project.bubble(bubble).unwrap().line_spacing, 1.4);
        assert_eq!(
            project.bubble(bubble).unwrap().text_color,
            Some([40, 50, 60, 255])
        );
        assert_eq!(
            project.bubble(bubble).unwrap().text_alignment,
            TextAlignment::Left
        );
        assert_eq!(
            (
                project.bubble(bubble).unwrap().bold,
                project.bubble(bubble).unwrap().strikethrough,
                project.bubble(bubble).unwrap().underline,
            ),
            (true, true, true)
        );
        let restored: ComicDubsProject =
            serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
        let restored = restored.bubble(bubble).unwrap();
        assert!((restored.bold, restored.strikethrough, restored.underline) == (true, true, true));
    }

    #[test]
    fn settings_supply_new_bubble_defaults() {
        let mut project = ComicDubsProject::default();
        assert_eq!((project.bubble_gap_ms(), project.page_gap_ms()), (250, 250));
        assert!(project.set_settings(Some("Comic Sans MS".into()), 500, 750, 32.0));
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        assert_eq!(project.font_family(), Some("Comic Sans MS"));
        assert_eq!(project.bubble(bubble).unwrap().font_size, 32.0);

        let json = serde_json::to_string(&project).unwrap();
        let restored: ComicDubsProject = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.font_family(), Some("Comic Sans MS"));
        assert_eq!(
            (restored.bubble_gap_ms(), restored.page_gap_ms()),
            (500, 750)
        );
        assert_eq!(restored.default_font_size(), 32.0);
    }

    #[test]
    fn bubbles_may_have_no_text() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        assert!(project.bubble(bubble).unwrap().text.is_empty());
        assert!(project.set_bubble_text(bubble, "Texte".into()));
        assert_eq!(
            bubble_playback_state(project.bubble(bubble).unwrap(), 0, 0),
            (true, false)
        );
        assert_eq!(
            bubble_playback_state(project.bubble(bubble).unwrap(), 0, 1),
            (true, true)
        );
        assert!(project.set_bubble_text(bubble, String::new()));
        assert!(project.validate().is_ok());
        assert_eq!(
            bubble_playback_state(project.bubble(bubble).unwrap(), 0, 0),
            (true, false)
        );
        assert_eq!(
            bubble_playback_state(project.bubble(bubble).unwrap(), 0, 1),
            (false, false)
        );
    }

    #[test]
    fn vertex_keyframes_are_sorted_and_use_step_interpolation() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        let middle = vec![
            Point { x: 0.2, y: 0.2 },
            Point { x: 0.8, y: 0.2 },
            Point { x: 0.5, y: 0.8 },
        ];
        let start = vec![
            Point { x: 0.3, y: 0.3 },
            Point { x: 0.7, y: 0.3 },
            Point { x: 0.5, y: 0.7 },
        ];
        assert!(project.set_bubble_vertex_keyframe(bubble, 1_000, middle.clone()));
        assert!(project.set_bubble_vertex_keyframe(bubble, 0, start.clone()));
        let bubble = project.bubble(bubble).unwrap();
        assert_eq!(bubble.points_at(999), start);
        assert_eq!(bubble.points_at(1_000), middle);
        assert_eq!(bubble.vertex_animation_duration_ms(), 1_000);
    }

    #[test]
    fn untouched_studio_fields_keep_the_legacy_json() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        project.add_bubble(page, triangle()).unwrap();
        let json = serde_json::to_value(&project).unwrap();
        assert!(json.get("studio").is_none());
        assert!(json["pages"][0].get("fx").is_none());
        let bubble = &json["pages"][0]["bubbles"][0];
        assert!(bubble.get("fx").is_none() && bubble.get("look").is_none());
        assert!(bubble.get("sound").is_none());

        let bubble_id = project.pages()[0].bubbles[0].id;
        assert!(project.apply_bubble_preset(bubble_id, BubblePreset::Shout));
        let restored: ComicDubsProject =
            serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
        let restored = restored.bubble(bubble_id).unwrap();
        assert!(restored.bold);
        assert_eq!(restored.fx.emphasis, BubbleEmphasis::Shake);
        assert_eq!(restored.fx.screen_effect, ScreenEffect::Shake);
    }

    #[test]
    fn duplicate_paste_and_style_copy_keep_ids_unique() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let other = project.add_page("2.png".into(), "2.png".into(), 10, 10);
        let source = project.add_bubble(page, triangle()).unwrap();
        let target = project.add_bubble(page, triangle()).unwrap();
        project.set_bubble_text(source, "Source".into());
        project.apply_bubble_preset(source, BubblePreset::Narration);
        let copy = project.duplicate_bubble(source).unwrap();
        assert_eq!(project.page(page).unwrap().bubbles[1].id, copy);
        assert_eq!(project.bubble(copy).unwrap().text, "Source");
        assert!(project.bubble(copy).unwrap().points[0].x > 0.1);

        let clipboard = project.bubble(source).unwrap().clone();
        let pasted = project
            .paste_bubble(other, &clipboard, Point { x: 0.0, y: 0.0 }, None)
            .unwrap();
        assert_eq!(project.page_of_bubble(pasted), Some(other));
        assert!(project.copy_bubble_style(target, &clipboard));
        assert_eq!(project.bubble(target).unwrap().color, [255, 238, 170, 255]);
        assert!(project.bubble(target).unwrap().text.is_empty());
        assert!(project.validate().is_ok());
    }

    #[test]
    fn vertex_edits_keep_animated_poses_consistent() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("1.png".into(), "1.png".into(), 10, 10);
        let bubble = project.add_bubble(page, triangle()).unwrap();
        project.set_bubble_vertex_keyframe(
            bubble,
            500,
            vec![
                Point { x: 0.2, y: 0.2 },
                Point { x: 0.8, y: 0.2 },
                Point { x: 0.5, y: 0.8 },
            ],
        );
        assert!(project.insert_bubble_vertex(bubble, 0, Point { x: 0.5, y: 0.05 }));
        assert_eq!(project.bubble(bubble).unwrap().points.len(), 4);
        assert_eq!(
            project.bubble(bubble).unwrap().vertex_keyframes[0]
                .points
                .len(),
            4
        );
        assert!(project.remove_bubble_vertex(bubble, 1));
        assert!(!project.remove_bubble_vertex(bubble, 0));
        assert!(project.add_bubble_tail(bubble));
        assert!(project.smooth_bubble(bubble));
        let shaped = project.bubble(bubble).unwrap();
        assert_eq!(shaped.points.len(), shaped.vertex_keyframes[0].points.len());
        assert!(project.translate_bubble(bubble, Point { x: 5.0, y: 0.0 }));
        assert!(project
            .bubble(bubble)
            .unwrap()
            .points
            .iter()
            .all(|point| point.x <= 1.0));
        assert!(project.validate().is_ok());
    }

    #[test]
    fn scripts_round_trip_and_respect_page_markers() {
        let mut project = ComicDubsProject::default();
        let first = project.add_page("un.png".into(), "un.png".into(), 10, 10);
        let second = project.add_page("deux.png".into(), "deux.png".into(), 10, 10);
        let a = project.add_bubble(first, triangle()).unwrap();
        let b = project.add_bubble(first, triangle()).unwrap();
        let c = project.add_bubble(second, triangle()).unwrap();
        project.set_bubble_text(a, "Bonjour".into());
        project.set_bubble_text(c, "Au revoir".into());
        let script = project.script_text();
        assert!(script.contains("# Page 2 — deux.png\nAu revoir\n"));

        let mut copy = project.clone();
        for id in [a, b, c] {
            copy.set_bubble_text(id, "x".into());
        }
        assert_eq!(copy.apply_script(&script), (3, 3));
        assert_eq!(copy.bubble(a).unwrap().text, "Bonjour");
        assert!(copy.bubble(b).unwrap().text.is_empty());
        assert_eq!(copy.bubble(c).unwrap().text, "Au revoir");

        // Extra lines of a page never spill onto the next one.
        let (filled, _) = copy.apply_script("# Page 1\nUn\nDeux\nTrois\n# Page 2\nQuatre");
        assert_eq!(filled, 3);
        assert_eq!(copy.bubble(c).unwrap().text, "Quatre");
        // Without markers, lines follow the global reading order.
        copy.apply_script("A\nB\nC");
        assert_eq!(copy.bubble(c).unwrap().text, "C");
    }

    #[test]
    fn studio_settings_reject_unknown_music_and_follow_audio_removal() {
        let mut project = ComicDubsProject::default();
        let audio = project.add_audio(
            "music.flac".into(),
            "music.flac".into(),
            RecordedAudio {
                file_name: "music.flac".into(),
                sample_rate: 48_000,
                channels: 2,
                sample_count: 48_000,
                checksum: "a".repeat(40),
                waveform: WaveformData::default(),
            },
        );
        assert!(!project.set_studio(StudioSettings {
            music_audio_id: Some(audio + 100),
            ..StudioSettings::default()
        }));
        assert!(project.set_studio(StudioSettings {
            music_audio_id: Some(audio),
            music_volume: 4.0,
            ..StudioSettings::default()
        }));
        assert_eq!(project.studio().music_volume, 1.0);
        assert!(project.remove_audio(audio));
        assert_eq!(project.studio().music_audio_id, None);
        assert_eq!(BubbleEntrance::Cut.cycled(-1), BubbleEntrance::Drop);
        assert_eq!(PageTransition::Cut.cycled(1), PageTransition::FadeBlack);
    }

    #[test]
    fn shots_are_ordered_framed_and_cleaned_up() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("p.png".into(), "p.png".into(), 900, 1_600);
        let bubble = project
            .add_bubble(
                page,
                vec![
                    Point { x: 0.1, y: 0.1 },
                    Point { x: 0.3, y: 0.1 },
                    Point { x: 0.3, y: 0.2 },
                    Point { x: 0.1, y: 0.2 },
                ],
            )
            .unwrap();
        let whole = project.add_shot(page, None).unwrap();
        let later = project
            .add_shot(
                page,
                Some(Region {
                    x: 0.5,
                    y: 0.5,
                    width: 0.4,
                    height: 0.2,
                }),
            )
            .unwrap();
        assert!(project
            .add_shot(
                page,
                Some(Region {
                    x: 0.5,
                    y: 0.5,
                    width: 0.001,
                    height: 0.2,
                })
            )
            .is_none());
        // Framing a bubble inserts the shot right after the one showing it.
        let close = project.add_shot_around_bubble(bubble, 16.0 / 9.0).unwrap();
        let shots = &project.page(page).unwrap().shots;
        assert_eq!(
            shots.iter().map(|shot| shot.id).collect::<Vec<_>>(),
            vec![whole, close, later]
        );
        let region = shots[1].region.unwrap();
        let aspect = region.width * 900.0 / (region.height * 1_600.0);
        assert!((aspect - 16.0 / 9.0).abs() < 0.01, "{aspect}");
        assert!(region.contains(bubble_center(&project.bubble(bubble).unwrap().points)));
        assert_eq!(project.page(page).unwrap().bubble_shots(), vec![Some(1)]);

        assert!(project.move_shot(later, -2));
        assert_eq!(project.page(page).unwrap().shots[0].id, later);
        let mut settings = *project.shot(close).unwrap();
        settings.move_ms = 0;
        settings.hold_ms = 99_000;
        assert!(project.set_shot(settings));
        let stored = project.shot(close).unwrap();
        assert_eq!((stored.move_ms, stored.hold_ms), (100, 20_000));

        let mut fx = project.bubble(bubble).unwrap().fx;
        fx.shot = Some(whole);
        project.set_bubble_fx(bubble, fx);
        assert!(project.remove_shot(whole));
        assert_eq!(project.bubble(bubble).unwrap().fx.shot, None);
        assert_eq!(project.page_of_shot(close), Some(page));

        // Saved shots survive a round trip; dangling references are dropped.
        let mut json = serde_json::to_value(&project).unwrap();
        json["pages"][0]["bubbles"][0]["fx"] = serde_json::json!({ "shot": 424242 });
        let mut loaded: ComicDubsProject = serde_json::from_value(json).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.page(page).unwrap().shots.len(), 2);
        assert_eq!(loaded.bubble(bubble).unwrap().fx.shot, None);
    }

    #[test]
    fn fitted_regions_take_the_video_shape_and_stay_on_the_page() {
        let region = Region {
            x: 0.9,
            y: 0.0,
            width: 0.1,
            height: 0.1,
        }
        .fitted(1_000, 1_000, 2.0);
        assert!((region.width - 0.2).abs() < 0.001 && (region.height - 0.1).abs() < 0.001);
        assert!((region.x + region.width - 1.0).abs() < 0.001);
        let huge = Region {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        }
        .fitted(1_000, 1_000, 2.0);
        assert!((huge.width - 1.0).abs() < 0.001 && (huge.height - 0.5).abs() < 0.001);
    }
}
