//! Comic Dubs timeline and effect engine.
//!
//! The editor preview and the video export both read this module, so what the
//! user scrubs in the studio is exactly what the MP4 contains: bubble timing,
//! camera moves, page transitions, entrance animations, emphasis, screen
//! effects and typewriter reveals are all pure functions of the time.

use crate::comic_dubs::{
    Bubble, BubbleEmphasis, BubbleEntrance, ComicAudioId, ComicDubsProject, Page, PageMotion,
    PageTransition, Point, Region, ScreenEffect, ShotMovement, TextReveal, EMPTY_SHOT_HOLD_MS,
};
use unicode_segmentation::UnicodeSegmentation;

/// Fade length of bubbles marked "disappears after its line".
const EXIT_MS: u64 = 250;

#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    pub bubble_index: usize,
    /// Slot start (equal to the reveal: camera moves belong to shots).
    pub start_ms: u64,
    /// The bubble text (or the whole bubble) appears; SFX and screen effects fire.
    pub reveal_ms: u64,
    pub voice_start_ms: u64,
    pub voice_ms: u64,
    pub voice_audio: Option<ComicAudioId>,
    pub sfx_audio: Option<ComicAudioId>,
    /// Duration of a typewriter / word reveal, 0 when instant.
    pub reveal_duration_ms: u64,
    pub speak_end_ms: u64,
    pub end_ms: u64,
}

/// A camera shot on the timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotCue {
    pub shot_index: usize,
    /// The camera starts moving toward the shot.
    pub start_ms: u64,
    /// The shot is fully framed.
    pub arrive_ms: u64,
    /// The next shot starts, or the page ends.
    pub end_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageSpan {
    pub page_index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub transition_ms: u64,
    pub cues: Vec<Cue>,
    pub shots: Vec<ShotCue>,
}

/// One beat of a page, in playback order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Shot(usize),
    Bubble(usize),
}

/// Playback order of a page: without shots, its bubbles in reading order;
/// with shots, each shot followed by the bubbles it frames.
pub fn page_steps(page: &Page) -> Vec<Step> {
    if page.shots.is_empty() {
        return (0..page.bubbles.len()).map(Step::Bubble).collect();
    }
    let assignment = page.bubble_shots();
    let mut steps = Vec::with_capacity(page.shots.len() + page.bubbles.len());
    for shot in 0..page.shots.len() {
        steps.push(Step::Shot(shot));
        steps.extend(
            assignment
                .iter()
                .enumerate()
                .filter(|(_, assigned)| **assigned == Some(shot))
                .map(|(bubble, _)| Step::Bubble(bubble)),
        );
    }
    steps
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timeline {
    pub pages: Vec<PageSpan>,
    pub total_ms: u64,
}

impl Timeline {
    /// Plans the ordered playback. Without `page_filter`, pages without
    /// bubbles or shots are skipped, like the historical player.
    pub fn build(project: &ComicDubsProject, page_filter: Option<usize>, minimum_ms: u64) -> Self {
        let playable = project
            .pages()
            .iter()
            .enumerate()
            .filter(|(index, page)| {
                page_filter.map_or(
                    !page.bubbles.is_empty() || !page.shots.is_empty(),
                    |filter| filter == *index,
                )
            })
            .collect::<Vec<_>>();
        let minimum_ms = minimum_ms.max(1);
        let mut elapsed = 0_u64;
        let mut pages = Vec::with_capacity(playable.len());
        for (position, (page_index, page)) in playable.iter().enumerate() {
            let start_ms = elapsed;
            let transition_ms = page.fx.effective_transition_ms();
            let lead_ms = page.fx.intro_ms.max(transition_ms);
            let last_page = position + 1 == playable.len();
            let page_gap_ms = if last_page {
                project.page_gap_ms().max(1_000)
            } else {
                project.page_gap_ms()
            };
            let steps = page_steps(page);
            if steps.is_empty() {
                let end_ms = start_ms + lead_ms + project.page_gap_ms().max(1_000);
                pages.push(PageSpan {
                    page_index: *page_index,
                    start_ms,
                    end_ms,
                    transition_ms,
                    cues: Vec::new(),
                    shots: Vec::new(),
                });
                elapsed = end_ms;
                continue;
            }
            let framed = page.bubble_shots();
            let mut cursor = start_ms + lead_ms;
            let mut cues = Vec::with_capacity(page.bubbles.len());
            let mut shots: Vec<ShotCue> = Vec::with_capacity(page.shots.len());
            for (step_index, step) in steps.iter().enumerate() {
                let last_step = step_index + 1 == steps.len();
                match *step {
                    Step::Shot(shot_index) => {
                        let shot = &page.shots[shot_index];
                        let move_ms = if shots.is_empty() || shot.movement == ShotMovement::Cut {
                            0
                        } else {
                            shot.move_ms
                        };
                        let hold_ms = if framed.contains(&Some(shot_index)) || shot.hold_ms > 0 {
                            shot.hold_ms
                        } else {
                            EMPTY_SHOT_HOLD_MS
                        };
                        if let Some(previous) = shots.last_mut() {
                            previous.end_ms = cursor;
                        }
                        shots.push(ShotCue {
                            shot_index,
                            start_ms: cursor,
                            arrive_ms: cursor + move_ms,
                            end_ms: cursor + move_ms + hold_ms,
                        });
                        cursor += move_ms + hold_ms;
                        if last_step {
                            cursor += page_gap_ms;
                        }
                    }
                    Step::Bubble(bubble_index) => {
                        let bubble = &page.bubbles[bubble_index];
                        let voice_audio = bubble.audio_id.filter(|id| project.audio(*id).is_some());
                        let voice_ms = voice_audio
                            .and_then(|id| project.audio(id))
                            .map_or(0, |audio| audio.duration_ms());
                        let reveal_ms = cursor;
                        let voice_start_ms = reveal_ms + bubble.sound.audio_delay_ms;
                        let reveal_duration_ms = match bubble.fx.text_reveal {
                            TextReveal::Instant => 0,
                            _ if voice_ms > 0 => voice_ms * 9 / 10,
                            _ => {
                                let characters = bubble.text.trim().graphemes(true).count() as f32;
                                (characters / project.studio().typewriter_cps.max(1.0) * 1_000.0)
                                    as u64
                            }
                        };
                        let entrance_ms = if bubble.fx.entrance == BubbleEntrance::Cut {
                            0
                        } else {
                            bubble.fx.entrance_ms
                        };
                        let speaking_ms = (bubble.sound.audio_delay_ms + voice_ms)
                            .max(bubble.vertex_animation_duration_ms())
                            .max(entrance_ms)
                            .max(bubble.sound.audio_delay_ms + reveal_duration_ms);
                        let speak_end_ms = reveal_ms + speaking_ms + bubble.sound.extra_hold_ms;
                        let gap_ms = if last_step {
                            page_gap_ms
                        } else {
                            project.bubble_gap_ms()
                        };
                        let end_ms = (speak_end_ms + gap_ms).max(cursor + minimum_ms);
                        cues.push(Cue {
                            bubble_index,
                            start_ms: cursor,
                            reveal_ms,
                            voice_start_ms,
                            voice_ms,
                            voice_audio,
                            sfx_audio: bubble
                                .sound
                                .sfx_audio_id
                                .filter(|id| project.audio(*id).is_some()),
                            reveal_duration_ms,
                            speak_end_ms,
                            end_ms,
                        });
                        cursor = end_ms;
                    }
                }
            }
            if let Some(last) = shots.last_mut() {
                last.end_ms = cursor;
            }
            pages.push(PageSpan {
                page_index: *page_index,
                start_ms,
                end_ms: cursor,
                transition_ms,
                cues,
                shots,
            });
            elapsed = cursor;
        }
        Self {
            pages,
            total_ms: elapsed,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// Index of the page span playing at `at_ms` (the last one past the end).
    pub fn span_index_at(&self, at_ms: u64) -> Option<usize> {
        if self.pages.is_empty() {
            return None;
        }
        Some(
            self.pages
                .partition_point(|span| span.end_ms <= at_ms)
                .min(self.pages.len() - 1),
        )
    }

    pub fn span_for_page(&self, page_index: usize) -> Option<&PageSpan> {
        self.pages.iter().find(|span| span.page_index == page_index)
    }

    pub fn cue_for(&self, page_index: usize, bubble_index: usize) -> Option<&Cue> {
        self.span_for_page(page_index)?
            .cues
            .iter()
            .find(|cue| cue.bubble_index == bubble_index)
    }

    /// Every cue in playback order with its page index.
    pub fn cues(&self) -> impl Iterator<Item = (usize, &Cue)> {
        self.pages
            .iter()
            .flat_map(|span| span.cues.iter().map(move |cue| (span.page_index, cue)))
    }

    /// Voice intervals used to duck the background music.
    pub fn voice_intervals(&self) -> Vec<(u64, u64)> {
        let mut intervals: Vec<(u64, u64)> = Vec::new();
        for (_, cue) in self.cues().filter(|(_, cue)| cue.voice_ms > 0) {
            let interval = (cue.voice_start_ms, cue.voice_start_ms + cue.voice_ms);
            match intervals.last_mut() {
                Some(last) if interval.0 <= last.1 + 400 => last.1 = last.1.max(interval.1),
                _ => intervals.push(interval),
            }
        }
        intervals
    }

    /// Reveal times of every cue, for previous/next navigation.
    pub fn reveal_marks(&self) -> Vec<u64> {
        self.cues().map(|(_, cue)| cue.reveal_ms).collect()
    }
}

/// Page-normalized point placed at the center of the view, and the zoom
/// relative to "whole page fitted in the view".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub cx: f32,
    pub cy: f32,
    pub zoom: f32,
}

impl Camera {
    pub const FULL: Self = Self {
        cx: 0.5,
        cy: 0.5,
        zoom: 1.0,
    };

    /// Frames `region` inside a view of aspect `view_aspect` (width / height).
    pub fn framing(region: Region, page_w: f32, page_h: f32, view_aspect: f32) -> Self {
        let fit = (view_aspect / page_w).min(1.0 / page_h);
        let needed = (view_aspect / (region.width * page_w)).min(1.0 / (region.height * page_h));
        let center = region.center();
        Self {
            cx: center.x,
            cy: center.y,
            zoom: (needed / fit.max(f32::EPSILON)).clamp(1.0, 6.0),
        }
    }

    /// Framing of a shot (`None` region = whole page), kept on the page.
    pub fn for_shot(region: Option<Region>, page_w: f32, page_h: f32, view_aspect: f32) -> Self {
        match region {
            Some(region) => Self::framing(region, page_w, page_h, view_aspect).clamped(
                page_w,
                page_h,
                view_aspect,
            ),
            None => Self::FULL,
        }
    }

    /// Page-normalized area this camera shows in a view of `view_aspect`
    /// (it can extend past the page when the page is letterboxed).
    pub fn visible_region(self, page_w: f32, page_h: f32, view_aspect: f32) -> Region {
        let fit = (view_aspect / page_w).min(1.0 / page_h);
        let scale = fit * self.zoom;
        let half_w = view_aspect * 0.5 / (page_w * scale);
        let half_h = 0.5 / (page_h * scale);
        Region {
            x: self.cx - half_w,
            y: self.cy - half_h,
            width: half_w * 2.0,
            height: half_h * 2.0,
        }
    }

    fn lerp(self, other: Self, ratio: f32) -> Self {
        Self {
            cx: self.cx + (other.cx - self.cx) * ratio,
            cy: self.cy + (other.cy - self.cy) * ratio,
            zoom: (self.zoom.ln() + (other.zoom.ln() - self.zoom.ln()) * ratio).exp(),
        }
    }

    /// Keeps the view on the page whenever the page covers it.
    pub fn clamped(self, page_w: f32, page_h: f32, view_aspect: f32) -> Self {
        let fit = (view_aspect / page_w).min(1.0 / page_h);
        let scale = fit * self.zoom;
        let half_w = view_aspect * 0.5 / (page_w * scale);
        let half_h = 0.5 / (page_h * scale);
        let clamp = |center: f32, half: f32| {
            if half >= 0.5 {
                0.5
            } else {
                center.clamp(half, 1.0 - half)
            }
        };
        Self {
            cx: clamp(self.cx, half_w),
            cy: clamp(self.cy, half_h),
            zoom: self.zoom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BubbleFrame {
    pub bubble_index: usize,
    /// Time inside the bubble's own vertex animation.
    pub pose_ms: u64,
    pub show_background: bool,
    pub show_text: bool,
    /// Whole-bubble transform (page units, around the bubble centroid).
    pub opacity: f32,
    pub scale: f32,
    pub offset: (f32, f32),
    /// Extra text transform (page units, around the text block center).
    pub text_opacity: f32,
    pub text_scale: f32,
    pub text_offset: (f32, f32),
    /// Revealed graphemes; `None` shows the whole text.
    pub reveal: Option<usize>,
    pub speaking: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerFrame {
    pub page_index: usize,
    pub camera: Camera,
    /// Layer offset as a fraction of the view width and height.
    pub offset: (f32, f32),
    /// Extra scale around the view center (zoom transitions, impacts).
    pub scale: f32,
    pub opacity: f32,
    /// 1 = normal, 0 = black (fade through black).
    pub brightness: f32,
    pub bubbles: Vec<BubbleFrame>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    /// Drawn in order, the last one on top.
    pub layers: Vec<LayerFrame>,
    /// White flash opacity drawn above everything.
    pub flash: f32,
    /// Screen shake as a fraction of the view height.
    pub shake: (f32, f32),
}

impl Frame {
    /// Quantized fingerprint: two frames with the same key render the same
    /// pixels, which lets the export reuse unchanged frames.
    pub fn key(&self) -> Vec<i64> {
        let q = |value: f32| (value * 10_000.0).round() as i64;
        let mut key = vec![q(self.flash), q(self.shake.0), q(self.shake.1)];
        for layer in &self.layers {
            key.extend([
                layer.page_index as i64,
                q(layer.camera.cx),
                q(layer.camera.cy),
                q(layer.camera.zoom),
                q(layer.offset.0),
                q(layer.offset.1),
                q(layer.scale),
                q(layer.opacity),
                q(layer.brightness),
            ]);
            for bubble in &layer.bubbles {
                key.extend([
                    bubble.bubble_index as i64,
                    bubble.pose_ms.min(i64::MAX as u64) as i64,
                    i64::from(bubble.show_background),
                    i64::from(bubble.show_text),
                    q(bubble.opacity),
                    q(bubble.scale),
                    q(bubble.offset.0),
                    q(bubble.offset.1),
                    q(bubble.text_opacity),
                    q(bubble.text_scale),
                    q(bubble.text_offset.0),
                    q(bubble.text_offset.1),
                    bubble.reveal.map_or(-1, |count| count as i64),
                ]);
            }
        }
        key
    }
}

/// Evaluates every visual at `at_ms` for a view of aspect `view_aspect`.
pub fn evaluate(
    project: &ComicDubsProject,
    timeline: &Timeline,
    at_ms: u64,
    view_aspect: f32,
) -> Frame {
    let Some(span_index) = timeline.span_index_at(at_ms) else {
        return Frame::default();
    };
    let at_ms = at_ms.min(timeline.total_ms.saturating_sub(1));
    let span = &timeline.pages[span_index];
    let view_aspect = if view_aspect.is_finite() && view_aspect > 0.0 {
        view_aspect
    } else {
        16.0 / 9.0
    };
    let mut current = evaluate_layer(project, span, at_ms, view_aspect);
    let mut frame = Frame::default();

    // Screen effects fired by bubbles of the current page.
    let mut punch = 1.0_f32;
    if let Some(page) = project.pages().get(span.page_index) {
        for cue in &span.cues {
            let Some(bubble) = page.bubbles.get(cue.bubble_index) else {
                continue;
            };
            let duration = bubble.fx.screen_effect_ms.max(1);
            if bubble.fx.screen_effect == ScreenEffect::None
                || at_ms < cue.reveal_ms
                || at_ms >= cue.reveal_ms + duration
            {
                continue;
            }
            let remaining = 1.0 - (at_ms - cue.reveal_ms) as f32 / duration as f32;
            let noise = noise2(at_ms, cue.bubble_index as u32 + 7);
            match bubble.fx.screen_effect {
                ScreenEffect::None => {}
                ScreenEffect::Shake => {
                    frame.shake.0 += noise.0 * 0.014 * remaining;
                    frame.shake.1 += noise.1 * 0.014 * remaining;
                }
                ScreenEffect::Flash => {
                    frame.flash = frame.flash.max(0.85 * remaining * remaining);
                }
                ScreenEffect::Impact => {
                    punch *= 1.0 + 0.14 * remaining * remaining * remaining;
                    frame.shake.0 += noise.0 * 0.008 * remaining;
                    frame.shake.1 += noise.1 * 0.008 * remaining;
                }
            }
        }
    }
    current.scale *= punch;

    let into_page = at_ms.saturating_sub(span.start_ms);
    if span.transition_ms > 0 && into_page < span.transition_ms {
        let progress = into_page as f32 / span.transition_ms as f32;
        let previous = span_index.checked_sub(1).map(|index| {
            let previous = &timeline.pages[index];
            evaluate_layer(
                project,
                previous,
                previous.end_ms.saturating_sub(1),
                view_aspect,
            )
        });
        let transition = project
            .pages()
            .get(span.page_index)
            .map_or(PageTransition::Cut, |page| page.fx.transition);
        match transition {
            PageTransition::Cut => frame.layers.push(current),
            PageTransition::FadeBlack => {
                if progress < 0.5 {
                    if let Some(mut previous) = previous {
                        previous.brightness = 1.0 - progress * 2.0;
                        frame.layers.push(previous);
                    }
                } else {
                    current.brightness = progress * 2.0 - 1.0;
                    frame.layers.push(current);
                }
            }
            PageTransition::CrossFade | PageTransition::Zoom => {
                let eased = ease_in_out(progress);
                if let Some(mut previous) = previous {
                    fade_bubbles(&mut previous, 1.0 - eased);
                    frame.layers.push(previous);
                }
                current.opacity = eased;
                if transition == PageTransition::Zoom {
                    current.scale *= 0.7 + 0.3 * ease_out(progress);
                }
                frame.layers.push(current);
            }
            PageTransition::SlideLeft | PageTransition::SlideUp => {
                let eased = ease_in_out(progress);
                let horizontal = transition == PageTransition::SlideLeft;
                if let Some(mut previous) = previous {
                    if horizontal {
                        previous.offset.0 -= eased;
                    } else {
                        previous.offset.1 -= eased;
                    }
                    frame.layers.push(previous);
                }
                if horizontal {
                    current.offset.0 += 1.0 - eased;
                } else {
                    current.offset.1 += 1.0 - eased;
                }
                frame.layers.push(current);
            }
            PageTransition::Flash => {
                frame.flash = frame.flash.max(0.95 * (1.0 - (progress * 2.0 - 1.0).abs()));
                if progress < 0.5 {
                    frame.layers.extend(previous);
                } else {
                    frame.layers.push(current);
                }
            }
        }
    } else {
        frame.layers.push(current);
    }
    frame
}

fn fade_bubbles(layer: &mut LayerFrame, opacity: f32) {
    for bubble in &mut layer.bubbles {
        bubble.opacity *= opacity;
    }
}

fn evaluate_layer(
    project: &ComicDubsProject,
    span: &PageSpan,
    at_ms: u64,
    view_aspect: f32,
) -> LayerFrame {
    let Some(page) = project.pages().get(span.page_index) else {
        return LayerFrame {
            page_index: span.page_index,
            camera: Camera::FULL,
            offset: (0.0, 0.0),
            scale: 1.0,
            opacity: 1.0,
            brightness: 1.0,
            bubbles: Vec::new(),
        };
    };
    LayerFrame {
        page_index: span.page_index,
        camera: camera_at(page, span, at_ms, view_aspect),
        offset: (0.0, 0.0),
        scale: 1.0,
        opacity: 1.0,
        brightness: 1.0,
        bubbles: page
            .bubbles
            .iter()
            .enumerate()
            .map(|(index, bubble)| {
                bubble_frame(
                    project,
                    bubble,
                    index,
                    span.cues.iter().find(|cue| cue.bubble_index == index),
                    at_ms,
                )
            })
            .collect(),
    }
}

fn camera_at(page: &Page, span: &PageSpan, at_ms: u64, view_aspect: f32) -> Camera {
    let (page_w, page_h) = (page.width.max(1) as f32, page.height.max(1) as f32);
    let resolve = |cue: &ShotCue| {
        let region = page.shots.get(cue.shot_index).and_then(|shot| shot.region);
        Camera::for_shot(region, page_w, page_h, view_aspect)
    };
    let current = span
        .shots
        .iter()
        .rposition(|cue| cue.start_ms <= at_ms)
        .unwrap_or(0);
    let mut camera = match span.shots.get(current) {
        None => Camera::FULL,
        Some(cue) if current == 0 || at_ms >= cue.arrive_ms => resolve(cue),
        Some(cue) => resolve(&span.shots[current - 1]).lerp(
            resolve(cue),
            ease_in_out(
                at_ms.saturating_sub(cue.start_ms) as f32
                    / cue.arrive_ms.saturating_sub(cue.start_ms).max(1) as f32,
            ),
        ),
    };

    let strength = page.fx.motion_strength;
    let length = span.end_ms.saturating_sub(span.start_ms).max(1);
    let progress = (at_ms.saturating_sub(span.start_ms) as f32 / length as f32).clamp(0.0, 1.0);
    let pan = |camera: &mut Camera, axis_x: bool, direction: f32| {
        camera.zoom *= 1.0 + 0.1 * strength;
        let shift = (progress - 0.5) * 0.08 * strength * direction / camera.zoom;
        if axis_x {
            camera.cx += shift;
        } else {
            camera.cy += shift;
        }
    };
    match page.fx.motion {
        PageMotion::None => {}
        PageMotion::ZoomIn => camera.zoom *= 1.0 + 0.12 * strength * progress,
        PageMotion::ZoomOut => camera.zoom *= 1.0 + 0.12 * strength * (1.0 - progress),
        PageMotion::PanRight => pan(&mut camera, true, 1.0),
        PageMotion::PanLeft => pan(&mut camera, true, -1.0),
        PageMotion::PanDown => pan(&mut camera, false, 1.0),
        PageMotion::PanUp => pan(&mut camera, false, -1.0),
    }
    camera.clamped(page_w, page_h, view_aspect)
}

fn bubble_frame(
    _project: &ComicDubsProject,
    bubble: &Bubble,
    bubble_index: usize,
    cue: Option<&Cue>,
    at_ms: u64,
) -> BubbleFrame {
    let has_text = !bubble.text.trim().is_empty();
    let revealed = cue.is_some_and(|cue| at_ms >= cue.reveal_ms);
    let whole = bubble.fx.whole_bubble;
    let (mask_background, show_text) =
        crate::comic_dubs::bubble_playback_state(bubble, 0, usize::from(revealed));
    let mut frame = BubbleFrame {
        bubble_index,
        pose_ms: 0,
        show_background: if whole { revealed } else { mask_background },
        show_text,
        opacity: 1.0,
        scale: 1.0,
        offset: (0.0, 0.0),
        text_opacity: 1.0,
        text_scale: 1.0,
        text_offset: (0.0, 0.0),
        reveal: None,
        speaking: false,
    };
    let Some(cue) = cue.filter(|_| revealed) else {
        return frame;
    };
    let local = at_ms - cue.reveal_ms;
    // Poses are stepped: keep only the active keyframe so still frames compare equal.
    frame.pose_ms = bubble
        .vertex_keyframes
        .iter()
        .rev()
        .find(|keyframe| keyframe.at_ms <= local)
        .map_or(0, |keyframe| keyframe.at_ms);
    frame.speaking = at_ms < cue.speak_end_ms;

    // Entrance.
    let progress = if bubble.fx.entrance == BubbleEntrance::Cut {
        1.0
    } else {
        (local as f32 / bubble.fx.entrance_ms.max(1) as f32).clamp(0.0, 1.0)
    };
    let (mut opacity, mut scale, mut offset) = (1.0, 1.0, (0.0, 0.0));
    match bubble.fx.entrance {
        BubbleEntrance::Cut => {}
        BubbleEntrance::Fade => opacity = progress,
        BubbleEntrance::Pop => {
            opacity = (progress * 3.0).min(1.0);
            scale = 0.2 + 0.8 * ease_out_back(progress);
        }
        BubbleEntrance::Zoom => {
            opacity = progress;
            scale = 1.6 - 0.6 * ease_out(progress);
        }
        BubbleEntrance::SlideUp => {
            opacity = progress;
            offset.1 = 0.06 * (1.0 - ease_out(progress));
        }
        BubbleEntrance::SlideDown => {
            opacity = progress;
            offset.1 = -0.06 * (1.0 - ease_out(progress));
        }
        BubbleEntrance::SlideLeft => {
            opacity = progress;
            offset.0 = 0.06 * (1.0 - ease_out(progress));
        }
        BubbleEntrance::SlideRight => {
            opacity = progress;
            offset.0 = -0.06 * (1.0 - ease_out(progress));
        }
        BubbleEntrance::Drop => {
            opacity = (progress * 4.0).min(1.0);
            offset.1 = -0.08 * (1.0 - ease_out_bounce(progress));
        }
    }

    // Exit after the line.
    if bubble.fx.exit_after && at_ms >= cue.speak_end_ms {
        let fade = 1.0 - (at_ms - cue.speak_end_ms) as f32 / EXIT_MS as f32;
        if fade <= 0.0 {
            if whole {
                frame.show_background = false;
            }
            frame.show_text = false;
        }
        opacity *= fade.max(0.0);
    }

    if whole {
        frame.opacity = opacity;
        frame.scale = scale;
        frame.offset = offset;
    } else {
        frame.text_opacity = opacity;
        frame.text_scale = scale;
        frame.text_offset = offset;
    }

    // Emphasis while the line plays.
    if frame.speaking {
        let strength = bubble.fx.emphasis_strength;
        let time = local as f32;
        match bubble.fx.emphasis {
            BubbleEmphasis::None => {}
            BubbleEmphasis::Shake => {
                let noise = noise2(at_ms, bubble_index as u32);
                frame.offset.0 += noise.0 * 0.004 * strength;
                frame.offset.1 += noise.1 * 0.004 * strength;
            }
            BubbleEmphasis::Pulse => {
                frame.scale *=
                    1.0 + 0.045 * strength * (time / 600.0 * std::f32::consts::TAU).sin().abs();
            }
            BubbleEmphasis::Float => {
                frame.offset.1 += 0.006 * strength * (time / 1_600.0 * std::f32::consts::TAU).sin();
            }
            BubbleEmphasis::Bounce => {
                frame.offset.1 -=
                    0.01 * strength * (time / 380.0 * std::f32::consts::PI).sin().abs();
            }
        }
    }

    // Typewriter / word reveal.
    if bubble.fx.text_reveal != TextReveal::Instant && has_text {
        let start = cue.voice_start_ms;
        let text = bubble.text.trim();
        let total = text.graphemes(true).count();
        frame.reveal = Some(if at_ms < start {
            0
        } else if cue.reveal_duration_ms == 0 {
            total
        } else {
            let progress = ((at_ms - start) as f32 / cue.reveal_duration_ms as f32).clamp(0.0, 1.0);
            match bubble.fx.text_reveal {
                TextReveal::Words => {
                    let words = text.split_whitespace().count().max(1);
                    let shown = (progress * words as f32).ceil() as usize;
                    graphemes_through_word(text, shown)
                }
                _ => (progress * total as f32).ceil() as usize,
            }
        })
        .filter(|count| *count < total);
    }
    frame
}

fn graphemes_through_word(text: &str, words: usize) -> usize {
    let mut seen = 0;
    let mut end = 0;
    let mut in_word = false;
    for (index, grapheme) in text.graphemes(true).enumerate() {
        let space = grapheme.chars().all(char::is_whitespace);
        if !space {
            if !in_word {
                seen += 1;
                if seen > words {
                    break;
                }
            }
            end = index + 1;
        }
        in_word = !space;
    }
    end
}

/// The first `reveal` graphemes of `text` (all of it when `None`).
pub fn revealed_prefix(text: &str, reveal: Option<usize>) -> &str {
    let Some(count) = reveal else {
        return text;
    };
    text.grapheme_indices(true)
        .nth(count)
        .map_or(text, |(index, _)| &text[..index])
}

/// Maps page-normalized coordinates to view pixels for one layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Screen position of the page origin.
    pub x: f32,
    pub y: f32,
    /// Screen size of the whole page.
    pub width: f32,
    pub height: f32,
}

impl Placement {
    /// `view` is `(x, y, width, height)` in pixels.
    pub fn compute(
        view: (f32, f32, f32, f32),
        page_w: u32,
        page_h: u32,
        layer: &LayerFrame,
        shake: (f32, f32),
    ) -> Self {
        let (page_w, page_h) = (page_w.max(1) as f32, page_h.max(1) as f32);
        let fit = (view.2 / page_w).min(view.3 / page_h);
        let scale = fit * layer.camera.zoom * layer.scale;
        let width = page_w * scale;
        let height = page_h * scale;
        let center_x = view.0 + view.2 * (0.5 + layer.offset.0) + shake.0 * view.3;
        let center_y = view.1 + view.3 * (0.5 + layer.offset.1) + shake.1 * view.3;
        Self {
            x: center_x - layer.camera.cx * width,
            y: center_y - layer.camera.cy * height,
            width,
            height,
        }
    }

    pub fn map(&self, point: Point) -> (f32, f32) {
        (
            self.x + point.x * self.width,
            self.y + point.y * self.height,
        )
    }

    /// Pixel size of text defined for a 1080-pixel-tall page.
    pub fn font_scale(&self) -> f32 {
        self.height / 1_080.0
    }
}

/// Bubble outline at `pose_ms`, with the whole-bubble transform applied.
pub fn animated_points(bubble: &Bubble, frame: &BubbleFrame) -> Vec<Point> {
    let points = bubble.points_at(frame.pose_ms);
    if frame.scale == 1.0 && frame.offset == (0.0, 0.0) {
        return points.to_vec();
    }
    let count = points.len().max(1) as f32;
    let center = Point {
        x: points.iter().map(|point| point.x).sum::<f32>() / count,
        y: points.iter().map(|point| point.y).sum::<f32>() / count,
    };
    points
        .iter()
        .map(|point| Point {
            x: center.x + (point.x - center.x) * frame.scale + frame.offset.0,
            y: center.y + (point.y - center.y) * frame.scale + frame.offset.1,
        })
        .collect()
}

/// SubRip subtitles of the bubble texts, shown from their reveal to the next line.
pub fn srt(project: &ComicDubsProject, timeline: &Timeline) -> String {
    let mut output = String::new();
    let mut number = 0;
    for (page_index, cue) in timeline.cues() {
        let Some(bubble) = project
            .pages()
            .get(page_index)
            .and_then(|page| page.bubbles.get(cue.bubble_index))
        else {
            continue;
        };
        let text = bubble.text.trim();
        if text.is_empty() {
            continue;
        }
        number += 1;
        output.push_str(&format!(
            "{number}\n{} --> {}\n{text}\n\n",
            srt_time(cue.reveal_ms),
            srt_time(cue.end_ms.max(cue.reveal_ms + 1)),
        ));
    }
    output
}

fn srt_time(at_ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02},{:03}",
        at_ms / 3_600_000,
        at_ms / 60_000 % 60,
        at_ms / 1_000 % 60,
        at_ms % 1_000
    )
}

pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) * 0.5
    }
}

pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

fn ease_out_bounce(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let (n1, d1) = (7.5625, 2.75);
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}

/// Smooth deterministic noise in -1..=1 so preview and export shake alike.
fn noise2(at_ms: u64, seed: u32) -> (f32, f32) {
    let t = (at_ms % 1_000_000) as f32;
    let s = seed as f32 * 12.9898;
    (
        (t * 0.071 + s).sin() * 0.6 + (t * 0.173 + s * 1.7).sin() * 0.4,
        (t * 0.067 + s * 2.3).cos() * 0.6 + (t * 0.151 + s * 0.7).sin() * 0.4,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comic_dubs::{BubbleFx, BubbleSound, PageFx};
    use crate::recording::{RecordedAudio, WaveformData};

    fn square(x: f32, y: f32) -> Vec<Point> {
        vec![
            Point { x, y },
            Point { x: x + 0.2, y },
            Point {
                x: x + 0.2,
                y: y + 0.2,
            },
            Point { x, y: y + 0.2 },
        ]
    }

    fn audio(project: &mut ComicDubsProject, samples: u64) -> ComicAudioId {
        project.add_audio(
            "voice.flac".into(),
            "voice.flac".into(),
            RecordedAudio {
                file_name: "voice.flac".into(),
                sample_rate: 1_000,
                channels: 1,
                sample_count: samples,
                checksum: String::new(),
                waveform: WaveformData::default(),
            },
        )
    }

    #[test]
    fn default_timing_matches_the_historical_player() {
        let mut project = ComicDubsProject::default();
        project.set_gaps(250, 900);
        let page = project.add_page("p.png".into(), "p.png".into(), 100, 100);
        let voice = audio(&mut project, 500);
        let first = project.add_bubble(page, square(0.1, 0.1)).unwrap();
        project.assign_audio(first, Some(voice));
        project.add_bubble(page, square(0.5, 0.5)).unwrap();
        let timeline = Timeline::build(&project, None, 40);
        let cues = &timeline.pages[0].cues;
        assert_eq!((cues[0].reveal_ms, cues[0].end_ms), (0, 750));
        assert_eq!((cues[1].reveal_ms, cues[1].end_ms), (750, 1_750));
        assert_eq!(timeline.total_ms, 1_750);
        assert_eq!(timeline.voice_intervals(), vec![(0, 500)]);
    }

    #[test]
    fn transitions_and_intro_delay_the_first_shot_and_bubble() {
        let mut project = ComicDubsProject::default();
        project.set_gaps(100, 100);
        let page = project.add_page("p.png".into(), "p.png".into(), 100, 100);
        project.set_page_fx(
            page,
            PageFx {
                transition: PageTransition::FadeBlack,
                transition_ms: 800,
                intro_ms: 300,
                ..PageFx::default()
            },
        );
        project.add_bubble(page, square(0.1, 0.1)).unwrap();
        project.add_shot(
            page,
            Some(Region {
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 0.5,
            }),
        );
        let timeline = Timeline::build(&project, None, 40);
        let span = &timeline.pages[0];
        assert_eq!(span.shots[0].start_ms, 800);
        assert_eq!(span.shots[0].arrive_ms, 800);
        assert_eq!(span.cues[0].reveal_ms, 800);
        // The first shot is framed from the very start of the page.
        let dark = evaluate(&project, &timeline, 500, 1.0);
        assert!(dark.layers[0].brightness < 0.3);
        assert!(dark.layers[0].camera.zoom > 1.5);
        assert!(!evaluate(&project, &timeline, 700, 1.0).layers[0].bubbles[0].show_text);
    }

    #[test]
    fn shots_play_in_order_and_frame_their_bubbles() {
        let mut project = ComicDubsProject::default();
        project.set_gaps(100, 100);
        let page = project.add_page("p.png".into(), "p.png".into(), 100, 100);
        let first = project.add_bubble(page, square(0.1, 0.1)).unwrap();
        let second = project.add_bubble(page, square(0.6, 0.6)).unwrap();
        for id in [first, second] {
            project.set_bubble_text(id, "Texte".into());
        }
        let whole = project.add_shot(page, None).unwrap();
        let top = project
            .add_shot(
                page,
                Some(Region {
                    x: 0.0,
                    y: 0.0,
                    width: 0.5,
                    height: 0.5,
                }),
            )
            .unwrap();
        let bottom = project
            .add_shot(
                page,
                Some(Region {
                    x: 0.5,
                    y: 0.5,
                    width: 0.5,
                    height: 0.5,
                }),
            )
            .unwrap();
        let mut shot = *project.shot(top).unwrap();
        shot.move_ms = 500;
        project.set_shot(shot);
        let mut shot = *project.shot(bottom).unwrap();
        shot.movement = ShotMovement::Cut;
        project.set_shot(shot);
        let page_ref = &project.pages()[0];
        assert_eq!(page_ref.bubble_shots(), vec![Some(1), Some(2)]);
        assert_eq!(
            page_steps(page_ref),
            vec![
                Step::Shot(0),
                Step::Shot(1),
                Step::Bubble(0),
                Step::Shot(2),
                Step::Bubble(1)
            ]
        );
        let timeline = Timeline::build(&project, None, 40);
        let span = &timeline.pages[0];
        // The establishing shot frames no bubble: it holds on its own.
        assert_eq!(span.shots[0].end_ms, EMPTY_SHOT_HOLD_MS);
        assert_eq!(
            (span.shots[1].start_ms, span.shots[1].arrive_ms),
            (EMPTY_SHOT_HOLD_MS, EMPTY_SHOT_HOLD_MS + 500)
        );
        assert_eq!(span.cues[0].reveal_ms, EMPTY_SHOT_HOLD_MS + 500);
        // A cut shot starts right where the previous bubble ends.
        assert_eq!(span.shots[2].start_ms, span.cues[0].end_ms);
        assert_eq!(span.shots[2].arrive_ms, span.shots[2].start_ms);
        assert_eq!(span.cues[1].reveal_ms, span.shots[2].start_ms);

        let camera = |at_ms| evaluate(&project, &timeline, at_ms, 1.0).layers[0].camera;
        assert_eq!(camera(100), Camera::FULL);
        let moving = camera(EMPTY_SHOT_HOLD_MS + 250);
        let framed = camera(EMPTY_SHOT_HOLD_MS + 540);
        assert!(moving.zoom > 1.0 && moving.zoom < framed.zoom);
        assert!((framed.cx - 0.25).abs() < 0.01 && framed.zoom > 1.9);
        let cut = camera(span.shots[2].start_ms);
        assert!((cut.cx - 0.75).abs() < 0.01);

        // Choosing another shot for a bubble moves it in the reading order.
        let mut fx = project.bubble(second).unwrap().fx;
        fx.shot = Some(whole);
        project.set_bubble_fx(second, fx);
        assert_eq!(
            page_steps(&project.pages()[0]),
            vec![
                Step::Shot(0),
                Step::Bubble(1),
                Step::Shot(1),
                Step::Bubble(0),
                Step::Shot(2)
            ]
        );
    }

    #[test]
    fn pages_with_only_shots_are_played() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("p.png".into(), "p.png".into(), 100, 100);
        project.add_shot(page, None);
        let timeline = Timeline::build(&project, None, 40);
        assert_eq!(timeline.pages.len(), 1);
        assert!(timeline.total_ms >= EMPTY_SHOT_HOLD_MS);
    }

    #[test]
    fn entrance_emphasis_and_reveal_animate_over_time() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("p.png".into(), "p.png".into(), 100, 100);
        let voice = audio(&mut project, 2_000);
        let bubble = project.add_bubble(page, square(0.1, 0.1)).unwrap();
        project.set_bubble_text(bubble, "Bonjour tout le monde".into());
        project.assign_audio(bubble, Some(voice));
        project.set_bubble_fx(
            bubble,
            BubbleFx {
                entrance: BubbleEntrance::Pop,
                entrance_ms: 400,
                text_reveal: TextReveal::Typewriter,
                emphasis: BubbleEmphasis::Shake,
                ..BubbleFx::default()
            },
        );
        let timeline = Timeline::build(&project, None, 40);
        let early = evaluate(&project, &timeline, 50, 1.0).layers[0].bubbles[0];
        let later = evaluate(&project, &timeline, 1_000, 1.0).layers[0].bubbles[0];
        let done = evaluate(&project, &timeline, 2_500, 1.0).layers[0].bubbles[0];
        assert!(early.text_scale < 0.8);
        assert!(early.reveal.unwrap() < later.reveal.unwrap());
        assert_eq!(done.reveal, None);
        assert!(later.speaking && !done.speaking);
        assert!(later.offset != (0.0, 0.0));
        assert_eq!(done.offset, (0.0, 0.0));
    }

    #[test]
    fn whole_bubbles_stay_hidden_until_their_turn_and_can_exit() {
        let mut project = ComicDubsProject::default();
        project.set_gaps(200, 200);
        let page = project.add_page("p.png".into(), "p.png".into(), 100, 100);
        let first = project.add_bubble(page, square(0.1, 0.1)).unwrap();
        let second = project.add_bubble(page, square(0.5, 0.5)).unwrap();
        for id in [first, second] {
            project.set_bubble_text(id, "Texte".into());
        }
        project.set_bubble_fx(
            second,
            BubbleFx {
                whole_bubble: true,
                exit_after: true,
                ..BubbleFx::default()
            },
        );
        project.set_bubble_sound(
            second,
            BubbleSound {
                extra_hold_ms: 500,
                ..BubbleSound::default()
            },
        );
        let timeline = Timeline::build(&project, None, 40);
        let start = evaluate(&project, &timeline, 0, 1.0);
        assert!(start.layers[0].bubbles[0].show_text);
        assert!(!start.layers[0].bubbles[1].show_background);
        let reveal = timeline.pages[0].cues[1].reveal_ms;
        let shown = evaluate(&project, &timeline, reveal + 10, 1.0);
        assert!(shown.layers[0].bubbles[1].show_background);
        let gone = evaluate(&project, &timeline, timeline.total_ms - 1, 1.0);
        assert!(!gone.layers[0].bubbles[1].show_background);
    }

    #[test]
    fn slide_transition_shows_both_pages_and_frame_keys_detect_changes() {
        let mut project = ComicDubsProject::default();
        for name in ["1.png", "2.png"] {
            let page = project.add_page(name.into(), name.into(), 100, 150);
            project.add_bubble(page, square(0.2, 0.2)).unwrap();
        }
        let second = project.pages()[1].id;
        project.set_page_fx(
            second,
            PageFx {
                transition: PageTransition::SlideLeft,
                transition_ms: 1_000,
                ..PageFx::default()
            },
        );
        let timeline = Timeline::build(&project, None, 40);
        let start = timeline.pages[1].start_ms;
        let middle = evaluate(&project, &timeline, start + 500, 16.0 / 9.0);
        assert_eq!(middle.layers.len(), 2);
        assert!(middle.layers[0].offset.0 < 0.0 && middle.layers[1].offset.0 > 0.0);
        let still = evaluate(&project, &timeline, 10, 16.0 / 9.0);
        let still_later = evaluate(&project, &timeline, 20, 16.0 / 9.0);
        assert_eq!(still.key(), still_later.key());
        assert_ne!(still.key(), middle.key());
    }

    #[test]
    fn camera_framing_zooms_on_regions_and_stays_on_the_page() {
        let region = Region {
            x: 0.0,
            y: 0.0,
            width: 0.25,
            height: 0.25,
        };
        let camera = Camera::framing(region, 1_000.0, 1_500.0, 16.0 / 9.0);
        assert!(camera.zoom > 1.0);
        let clamped = camera.clamped(1_000.0, 1_500.0, 16.0 / 9.0);
        assert!(clamped.cy >= camera.cy);
        assert_eq!(
            Camera::FULL.clamped(1_000.0, 1_500.0, 16.0 / 9.0),
            Camera::FULL
        );
        // A region shaped like the video is exactly what the camera shows.
        let fitted = region.fitted(1_000, 1_500, 16.0 / 9.0);
        let visible = Camera::for_shot(Some(fitted), 1_000.0, 1_500.0, 16.0 / 9.0).visible_region(
            1_000.0,
            1_500.0,
            16.0 / 9.0,
        );
        for (a, b) in [
            (fitted.x, visible.x),
            (fitted.y, visible.y),
            (fitted.width, visible.width),
            (fitted.height, visible.height),
        ] {
            assert!((a - b).abs() < 0.001, "{fitted:?} vs {visible:?}");
        }
    }

    #[test]
    fn srt_and_word_reveal_helpers() {
        let mut project = ComicDubsProject::default();
        let page = project.add_page("p.png".into(), "p.png".into(), 100, 100);
        let bubble = project.add_bubble(page, square(0.1, 0.1)).unwrap();
        project.set_bubble_text(bubble, "Salut à toi".into());
        project.add_bubble(page, square(0.5, 0.5)).unwrap();
        let timeline = Timeline::build(&project, None, 40);
        let subtitles = srt(&project, &timeline);
        assert!(subtitles.starts_with("1\n00:00:00,000 --> 00:00:00,250\nSalut à toi"));
        assert_eq!(subtitles.matches("-->").count(), 1);
        assert_eq!(graphemes_through_word("Salut à toi", 2), 7);
        assert_eq!(revealed_prefix("Salut à toi", Some(7)), "Salut à");
        assert_eq!(revealed_prefix("Salut", None), "Salut");
    }
}
