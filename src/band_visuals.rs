//! Look of the band (BR) elements shared by the editor preview and both video
//! exports (GPU and CPU), so the exported band cannot drift from the editor.
//!
//! Sizes are in editor pixels; the exports multiply them by their band scale.
//! Colours are sRGB values: the editor converts band quads to linear before
//! drawing them on its sRGB surface, the exports write them as is.

use crate::ui::primitives::Rect;

// -- Loop ("boucle") marker --
pub const LOOP_MARKER_COLOR: [f32; 4] = [0.85, 0.15, 0.15, 0.9];
pub const MARKER_BAR_WIDTH: f32 = 2.0;
/// Length of each of the two bars of the loop "X".
pub const LOOP_MARKER_X_BAR_LENGTH: f32 = 20.0;
pub const LOOP_MARKER_X_THICKNESS: f32 = 2.5;
pub const LOOP_NUMBER_COLOR: [u8; 3] = [217, 38, 38];
pub const LOOP_NUMBER_FONT_SIZE: f32 = 18.0;
/// Top-left of the loop number, relative to the marker and the band centre.
pub const LOOP_NUMBER_OFFSET: [f32; 2] = [8.0, 5.0];

// -- Out marker --
pub const OUT_MARKER_COLOR: [f32; 4] = [0.85, 0.45, 0.45, 0.7];
/// Oblique bar length as a fraction of the band height.
pub const OUT_MARKER_BAR_LENGTH_RATIO: f32 = 0.25;
pub const OUT_MARKER_BAR_THICKNESS: f32 = 2.0;
pub const OUT_MARKER_BAR_ANGLE: f32 = 0.5;
pub const OUT_MARKER_BAR_OFFSETS: [f32; 2] = [-5.0, 5.0];
pub const OUT_LABEL: &str = "out";
pub const OUT_LABEL_COLOR: [u8; 3] = [220, 120, 120];
pub const OUT_LABEL_FONT_SIZE: f32 = 10.0;
/// Left edge of the "out" label, relative to the marker.
pub const OUT_LABEL_OFFSET_X: f32 = 12.0;

// -- Scene change and liaison markers --
pub const SCENE_CHANGE_COLOR: [f32; 4] = [0.9, 0.9, 0.95, 0.8];
pub const LIAISON_MARKER_TINT: [f32; 4] = [0.7, 0.7, 0.75, 0.9];

// -- Presence (Off / Partial) underline --
pub const PRESENCE_UNDERLINE_THICKNESS: f32 = 1.5;
/// Distance from the bottom of the line body to the top of the underline.
pub const PRESENCE_UNDERLINE_BOTTOM_OFFSET: f32 = 3.0;
pub const PRESENCE_DASH_LENGTH: f32 = 7.0;
/// Distance between the starts of two dashes.
pub const PRESENCE_DASH_PERIOD: f32 = 12.0;

// -- Voice actor icons beside a character label --
pub const ACTOR_ICON_GAP: f32 = 3.0;
pub const ACTOR_ICON_BG_TOP: [f32; 4] = [0.05, 0.05, 0.07, 0.92];
pub const ACTOR_ICON_BG_BOTTOM: [f32; 4] = [0.02, 0.02, 0.03, 0.92];
pub const ACTOR_ICON_BORDER: [f32; 4] = [0.75, 0.75, 0.85, 0.45];
pub const ACTOR_ICON_BORDER_WIDTH: f32 = 1.0;
pub const ACTOR_ICON_RADIUS: f32 = 3.0;
pub const ACTOR_FALLBACK_TEXT_COLOR: [u8; 3] = [230, 230, 238];
/// Font size of an actor's name shown instead of a missing icon, relative
/// to the icon size.
pub const ACTOR_FALLBACK_FONT_RATIO: f32 = 0.55;

// -- Breath arrows ("↑" / "↓" lines) --
pub const BREATH_ARROW_COLOR: [f32; 4] = [0.85, 0.85, 0.90, 0.9];
pub const BREATH_ARROW_MARGIN: f32 = 4.0;
pub const BREATH_ARROW_THICKNESS: f32 = 2.0;
pub const BREATH_ARROW_HEAD_LENGTH: f32 = 8.0;
/// Angle between the arrow shaft and each head bar (about 30 degrees).
pub const BREATH_ARROW_HEAD_SPREAD: f32 = 0.5;

// -- Text emotions --
/// Opacity of the small readable copy drawn in the text-emotion lane.
pub const TEXT_EMOTION_LANE_ALPHA: f32 = 0.82;

// -- Character label shrunk to fit beside another character's line --
/// Size lost per shrink step.
pub const BADGE_FIT_STEP: f32 = 0.01;
/// Last shrink step: the label never gets smaller than 5 % of its size.
pub const BADGE_FIT_MAX_STEPS: i32 = 95;

/// Converts an sRGB colour with float channels to 8-bit RGBA.
pub fn rgba8(color: [f32; 4]) -> [u8; 4] {
    color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Two ends of the bar of `length` centred on (`cx`, `cy`) and rotated by
/// `angle` like a rotated band quad.
pub fn rotated_bar_ends(cx: f32, cy: f32, length: f32, angle: f32) -> [(f32, f32); 2] {
    let (sin, cos) = angle.sin_cos();
    let half = length / 2.0;
    [
        (cx - cos * half, cy - sin * half),
        (cx + cos * half, cy + sin * half),
    ]
}

/// Centre, length and rotation of the bars of a breath arrow drawn in the
/// line body `rect` (`margin`, `head_length` already scaled): the shaft
/// first, then the two head bars.
pub fn breath_arrow_bars(
    rect: Rect,
    up: bool,
    margin: f32,
    head_length: f32,
) -> [(f32, f32, f32, f32); 3] {
    let dx = rect.width - margin * 2.0;
    let dy = rect.height - margin * 2.0;
    let length = (dx * dx + dy * dy).sqrt();
    let angle = if up { -dy.atan2(dx) } else { dy.atan2(dx) };
    let tip_x = rect.x + rect.width - margin;
    let tip_y = if up {
        rect.y + margin
    } else {
        rect.y + rect.height - margin
    };
    let base_angle = std::f32::consts::PI + angle;
    [
        (
            rect.x + rect.width / 2.0,
            rect.y + rect.height / 2.0,
            length,
            angle,
        ),
        (
            tip_x,
            tip_y,
            head_length,
            base_angle + BREATH_ARROW_HEAD_SPREAD,
        ),
        (
            tip_x,
            tip_y,
            head_length,
            base_angle - BREATH_ARROW_HEAD_SPREAD,
        ),
    ]
}

/// Character label placed after testing it against the other line bodies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BadgeFit {
    /// The label covers a line of the same character: it is not drawn.
    pub hidden: bool,
    pub rect: Rect,
    /// Size of the label relative to its normal size.
    pub scale: f32,
}

/// Fits the character label `badge` of a line starting at `line_x`.
///
/// `overlaps(rect, same_character_only)` tells whether `rect` covers the
/// body of another line (only those of the same character when the flag is
/// set). A label over a line of the same character is hidden. A label over
/// another character's line is moved against its own line (`gap` before
/// it), then shrunk by 1 % steps, down to 5 %, until it fits. It always
/// stays fully opaque.
pub fn fit_character_badge(
    badge: Rect,
    line_x: f32,
    gap: f32,
    mut overlaps: impl FnMut(&Rect, bool) -> bool,
) -> BadgeFit {
    if overlaps(&badge, true) {
        return BadgeFit {
            hidden: true,
            rect: badge,
            scale: 1.0,
        };
    }
    if !overlaps(&badge, false) {
        return BadgeFit {
            hidden: false,
            rect: badge,
            scale: 1.0,
        };
    }

    // Preserve the original full-size left alignment before considering a
    // shrink. After that move, every smaller badge is a strict subset of the
    // previous one, so collision is monotone in the shrink step.
    let fitted_at = |step: i32| {
        let scale = 1.0 - step as f32 * BADGE_FIT_STEP;
        Rect {
            x: line_x - gap - badge.width * scale,
            y: badge.y,
            width: badge.width * scale,
            height: badge.height * scale,
        }
    };
    let full_size_fitted = fitted_at(0);
    if !overlaps(&full_size_fitted, false) {
        return BadgeFit {
            hidden: false,
            rect: full_size_fitted,
            scale: 1.0,
        };
    }

    // Find the first collision-free step instead of testing all of them.
    let mut lo = 1;
    let mut hi = BADGE_FIT_MAX_STEPS;
    let mut best = BADGE_FIT_MAX_STEPS + 1; // sentinel: no collision-free step
    while lo <= hi {
        let mid = (lo + hi) / 2;
        if overlaps(&fitted_at(mid), false) {
            lo = mid + 1;
        } else {
            best = mid;
            hi = mid - 1;
        }
    }
    let step = best.min(BADGE_FIT_MAX_STEPS);
    BadgeFit {
        hidden: false,
        rect: fitted_at(step),
        scale: 1.0 - step as f32 * BADGE_FIT_STEP,
    }
}

/// The syllable the karaoke dot lands on rises by this much (at scale 1),
/// then settles.
pub const KARAOKE_SYLLABLE_LIFT: f32 = 2.0;
pub const KARAOKE_SYLLABLE_LIFT_SECONDS: f32 = 0.22;
/// Its letters shine meanwhile: a halo that follows their shape (the text
/// drawn again around itself, `KARAOKE_SYLLABLE_HALO_RADIUS` pixels away at
/// scale 1), and the sung letters whitened.
pub const KARAOKE_SYLLABLE_SHINE_SECONDS: f32 = 0.38;
pub const KARAOKE_SYLLABLE_SHINE_ALPHA: f32 = 0.45;
pub const KARAOKE_SYLLABLE_HALO_ALPHA: f32 = 0.16;
pub const KARAOKE_SYLLABLE_HALO_RADIUS: f32 = 1.25;

/// How high the syllable being sung is lifted, `elapsed` seconds after the
/// dot landed on it: a single small bounce.
pub fn karaoke_syllable_lift(elapsed: f32) -> f32 {
    let t = (elapsed / KARAOKE_SYLLABLE_LIFT_SECONDS).clamp(0.0, 1.0);
    KARAOKE_SYLLABLE_LIFT * (t * std::f32::consts::PI).sin().max(0.0)
}

/// Strength (1 to 0) of the light on the syllable being sung.
pub fn karaoke_syllable_shine(elapsed: f32) -> f32 {
    let t = (elapsed / KARAOKE_SYLLABLE_SHINE_SECONDS).clamp(0.0, 1.0);
    (1.0 - t) * (1.0 - t)
}

/// How a karaoke line is drawn while it is sung: the syllable the dot is on
/// apart from the rest, lifted and lit when the dot lands on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KaraokeSyllableEffect {
    /// Start and end of the syllable being sung, along the drawn text.
    pub span: (f32, f32),
    /// How much of the text is sung (0 to 1): it takes the character colour.
    pub filled: f32,
    /// How far the dot is into the syllable (0 to 1).
    pub local: f32,
    /// Seconds since the dot landed on the syllable.
    pub elapsed: f32,
    /// Lift of the syllable at scale 1, in pixels.
    pub lift: f32,
    /// Strength of the light on it (1 to 0).
    pub shine: f32,
}

/// The effect on `line` at `progress` (0 to 1) of its duration.
pub fn karaoke_syllable_effect(
    line: &crate::rythmo_line::RythmoLine,
    lang: &str,
    progress: f32,
    fps: f64,
) -> KaraokeSyllableEffect {
    let dot = line.karaoke_dot_state(lang, progress);
    let fps = if fps.is_finite() && fps > 0.0 {
        fps
    } else {
        24.0
    };
    let syllable_seconds = dot.share as f64 * line.duration_frames.max(1) as f64 / fps;
    let elapsed = (dot.local as f64 * syllable_seconds) as f32;
    KaraokeSyllableEffect {
        span: dot.span,
        filled: dot.position,
        local: dot.local,
        elapsed,
        lift: karaoke_syllable_lift(elapsed),
        shine: karaoke_syllable_shine(elapsed),
    }
}

/// One slice of the text of a sung karaoke line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KaraokeTextSlice {
    /// Start and end of the slice along the drawn text (0 to 1).
    pub from: f32,
    pub to: f32,
    /// Shift to the right and lift, in pixels at scale 1.
    pub shift_x: f32,
    pub lift: f32,
    /// Drawn in the character colour (else in white).
    pub coloured: bool,
    pub alpha: f32,
}

/// The slices a sung karaoke line is drawn in, in order. The shine is made
/// of the letters themselves: no shape is drawn behind the text.
pub fn karaoke_syllable_slices(effect: &KaraokeSyllableEffect) -> Vec<KaraokeTextSlice> {
    let (start, end) = effect.span;
    let filled = effect.filled.clamp(0.0, 1.0);
    let lift = effect.lift;
    let slice = |from: f32, to: f32, lift: f32, coloured: bool, alpha: f32| KaraokeTextSlice {
        from,
        to,
        shift_x: 0.0,
        lift,
        coloured,
        alpha,
    };
    let mut slices = Vec::new();
    if effect.shine > 0.0 {
        // The halo, under the text: the letters of the syllable drawn all
        // around their place, faintly.
        let radius = KARAOKE_SYLLABLE_HALO_RADIUS;
        let diagonal = radius * std::f32::consts::FRAC_1_SQRT_2;
        for (dx, dy) in [
            (-radius, 0.0),
            (radius, 0.0),
            (0.0, -radius),
            (0.0, radius),
            (-diagonal, -diagonal),
            (diagonal, -diagonal),
            (-diagonal, diagonal),
            (diagonal, diagonal),
        ] {
            slices.push(KaraokeTextSlice {
                from: start,
                to: end,
                shift_x: dx,
                lift: lift - dy,
                coloured: false,
                alpha: KARAOKE_SYLLABLE_HALO_ALPHA * effect.shine,
            });
        }
    }
    slices.extend([
        slice(0.0, start, 0.0, false, 1.0),
        slice(start, end, lift, false, 1.0),
        slice(end, 1.0, 0.0, false, 1.0),
        // What is already sung takes the colour of the character.
        slice(0.0, filled.min(start), 0.0, true, 1.0),
        slice(start, filled.min(end), lift, true, 1.0),
    ]);
    if effect.shine > 0.0 {
        // The sung letters of the syllable briefly whitened.
        slices.push(slice(
            start,
            filled.min(end),
            lift,
            false,
            KARAOKE_SYLLABLE_SHINE_ALPHA * effect.shine,
        ));
    }
    slices.retain(|slice| slice.to > slice.from);
    slices
}

/// Mark of a mute syllable to pronounce, shown during playback under the
/// words whose pin is on: same yellow as the pin.
pub const PRONOUNCE_MARK_SIZE: f32 = 9.0;
/// How far above the bottom of its row the mark starts: it sits under the
/// letters, clear of them, and overhangs the bottom edge of the row.
pub const PRONOUNCE_MARK_HEIGHT: f32 = 5.0;
pub const PRONOUNCE_MARK_COLOR: [f32; 4] = [1.0, 0.80, 0.16, 1.0];
pub const PRONOUNCE_MARK_RIM: [f32; 4] = [0.10, 0.08, 0.02, 1.0];

/// Top-left corner and size of the mark centred at `share` of the width of
/// a line drawn at `line_x`, `line_y`: across the bottom edge of its row,
/// under the syllable.
pub fn pronounce_mark_rect(
    line_x: f32,
    line_y: f32,
    line_width: f32,
    row_height: f32,
    share: f32,
    scale: f32,
) -> (f32, f32, f32) {
    let unit = scale.max(0.5);
    let size = PRONOUNCE_MARK_SIZE * unit;
    (
        line_x + share.clamp(0.0, 1.0) * line_width - size / 2.0,
        line_y + row_height - PRONOUNCE_MARK_HEIGHT * unit,
        size,
    )
}

/// Arrow that links the consonant of a liaison to the vowel after it.
pub const LIAISON_ARROW_THICKNESS: f32 = 2.5;
/// How far the curve hangs under its ends, and the length of its head.
pub const LIAISON_ARROW_DEPTH: f32 = 4.0;
pub const LIAISON_ARROW_HEAD: f32 = 6.0;
/// Height of the oblique stroke of a liaison that is not made.
pub const LIAISON_ARROW_SLASH: f32 = 10.0;
/// How far above the bottom of its row the arrow starts during playback:
/// its ends sit under the letters, clear of them, and its curve reaches the
/// bottom edge of the row.
pub const LIAISON_ARROW_HEIGHT: f32 = 4.0;
pub const LIAISON_ARROW_COLOR: [f32; 4] = PRONOUNCE_MARK_COLOR;

/// Strokes (`[x0, y0, x1, y1]`) of the arrow of a liaison, from the
/// consonant at `from_x` to the vowel at `to_x`: a shallow curve hanging
/// under `top`, its head on the vowel and, when the liaison is not made
/// (`crossed`), an oblique stroke across its middle.
pub fn liaison_arrow_strokes(
    from_x: f32,
    to_x: f32,
    top: f32,
    scale: f32,
    crossed: bool,
) -> Vec<[f32; 4]> {
    const STEPS: usize = 8;
    let unit = scale.max(0.5);
    let to_x = to_x.max(from_x + 2.0 * unit);
    let span = to_x - from_x;
    let depth = (LIAISON_ARROW_DEPTH * unit).min(span * 0.35);
    let middle_x = (from_x + to_x) / 2.0;
    // Quadratic curve from the consonant to the vowel, lowest in the middle.
    let point = |t: f32| -> (f32, f32) {
        let inverse = 1.0 - t;
        (
            inverse * inverse * from_x + 2.0 * inverse * t * middle_x + t * t * to_x,
            top + 2.0 * inverse * t * 2.0 * depth,
        )
    };
    let mut strokes = Vec::with_capacity(STEPS + 3);
    let mut previous = point(0.0);
    for step in 1..=STEPS {
        let next = point(step as f32 / STEPS as f32);
        strokes.push([previous.0, previous.1, next.0, next.1]);
        previous = next;
    }
    // The head, on the vowel: two strokes back along the end of the curve.
    let (dx, dy) = (to_x - middle_x, -2.0 * depth);
    let length = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
    let (back_x, back_y) = (-dx / length, -dy / length);
    let head = LIAISON_ARROW_HEAD * unit;
    for angle in [BREATH_ARROW_HEAD_SPREAD, -BREATH_ARROW_HEAD_SPREAD] {
        let (sin, cos) = angle.sin_cos();
        strokes.push([
            to_x,
            top,
            to_x + (back_x * cos - back_y * sin) * head,
            top + (back_x * sin + back_y * cos) * head,
        ]);
    }
    if crossed {
        let half_height = LIAISON_ARROW_SLASH * unit / 2.0;
        let half_width = half_height * 0.6;
        let middle_y = top + depth;
        strokes.push([
            middle_x - half_width,
            middle_y + half_height,
            middle_x + half_width,
            middle_y - half_height,
        ]);
    }
    strokes
}

/// Smallest size a label is shrunk to so that it stays on the band.
pub const BADGE_EDGE_MIN_SCALE: f32 = 0.35;

/// Keeps the label of a centred karaoke line on the band. A name too long
/// for the room between the left edge of the band (`min_x`) and its line
/// (`gap` before `line_x`) is shrunk against the line, down to
/// `BADGE_EDGE_MIN_SCALE`, then held at the edge rather than cut by it.
pub fn keep_badge_on_band(fit: BadgeFit, line_x: f32, gap: f32, min_x: f32) -> BadgeFit {
    if fit.hidden || fit.rect.x >= min_x || !(fit.scale > 0.0) {
        return fit;
    }
    let full_width = fit.rect.width / fit.scale;
    let full_height = fit.rect.height / fit.scale;
    if !(full_width > 0.0) {
        return fit;
    }
    let room = line_x - gap - min_x;
    let scale = (room / full_width)
        .min(fit.scale)
        .max(BADGE_EDGE_MIN_SCALE.min(fit.scale));
    let width = full_width * scale;
    BadgeFit {
        hidden: false,
        rect: Rect {
            x: (line_x - gap - width).max(min_x),
            y: fit.rect.y,
            width,
            height: full_height * scale,
        },
        scale,
    }
}

/// `fit_character_badge` for the exports: the label of line `line_id` of
/// `character_name` against the bodies of the other `lines` (line id to
/// body and character name).
pub fn fit_character_badge_among_lines(
    badge: Rect,
    line_x: f32,
    gap: f32,
    line_id: u64,
    character_name: &str,
    lines: &std::collections::HashMap<u64, (Rect, String)>,
) -> BadgeFit {
    fit_character_badge(badge, line_x, gap, |candidate, same_character_only| {
        lines.iter().any(|(&other_id, (other_rect, other_name))| {
            other_id != line_id
                && (!same_character_only || other_name == character_name)
                && rects_overlap(candidate, other_rect)
        })
    })
}

/// Whether two rectangles share some area.
pub fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn fit_against(badge: Rect, line_x: f32, others: &[(Rect, bool)]) -> BadgeFit {
        fit_character_badge(badge, line_x, 2.0, |candidate, same_only| {
            others
                .iter()
                .any(|(other, same)| (!same_only || *same) && rects_overlap(candidate, other))
        })
    }

    #[test]
    fn the_pronounce_mark_sits_under_the_letters_of_its_syllable() {
        let (x, y, size) = pronounce_mark_rect(100.0, 50.0, 200.0, 40.0, 0.5, 1.0);
        assert_eq!(size, PRONOUNCE_MARK_SIZE);
        // Centred under its syllable, starting near the bottom of the row
        // and overhanging it.
        assert!((x + size / 2.0 - 200.0).abs() < 1e-3);
        assert_eq!(y, 90.0 - PRONOUNCE_MARK_HEIGHT);
        assert!(y + size > 90.0);
        // It follows the scale of the band.
        let (_, scaled_y, scaled_size) = pronounce_mark_rect(100.0, 50.0, 200.0, 80.0, 0.5, 2.0);
        assert_eq!(scaled_size, PRONOUNCE_MARK_SIZE * 2.0);
        assert_eq!(scaled_y, 130.0 - PRONOUNCE_MARK_HEIGHT * 2.0);
    }

    #[test]
    fn the_liaison_arrow_runs_from_the_consonant_to_the_vowel() {
        let strokes = liaison_arrow_strokes(100.0, 140.0, 50.0, 1.0, false);
        // A continuous curve from the consonant to the vowel, hanging under
        // its ends, then the two strokes of the head on the vowel.
        assert_eq!(strokes.len(), 10);
        assert_eq!((strokes[0][0], strokes[0][1]), (100.0, 50.0));
        for pair in strokes[..8].windows(2) {
            assert_eq!((pair[0][2], pair[0][3]), (pair[1][0], pair[1][1]));
        }
        assert!((strokes[7][2] - 140.0).abs() < 1e-3 && (strokes[7][3] - 50.0).abs() < 1e-3);
        let lowest = strokes[..8].iter().map(|s| s[3]).fold(f32::MIN, f32::max);
        assert!((lowest - (50.0 + LIAISON_ARROW_DEPTH)).abs() < 1e-3);
        for head in &strokes[8..] {
            assert_eq!((head[0], head[1]), (140.0, 50.0));
            assert!(head[2] < 140.0 && head[3] >= 50.0 - 1.0);
            let length = ((head[2] - head[0]).powi(2) + (head[3] - head[1]).powi(2)).sqrt();
            assert!((length - LIAISON_ARROW_HEAD).abs() < 1e-3);
        }
        // Not made: the same arrow, with an oblique stroke across its middle.
        let crossed = liaison_arrow_strokes(100.0, 140.0, 50.0, 1.0, true);
        assert_eq!(crossed.len(), 11);
        assert_eq!(crossed[..10], strokes[..]);
        let slash = crossed[10];
        assert!(slash[0] < 120.0 && slash[2] > 120.0 && slash[1] > slash[3]);
        assert!(((slash[0] + slash[2]) / 2.0 - 120.0).abs() < 1e-3);
        // Two letters side by side still get an arrow.
        assert!(liaison_arrow_strokes(100.0, 100.5, 50.0, 1.0, false)
            .iter()
            .all(|stroke| stroke.iter().all(|value| value.is_finite())));
    }

    #[test]
    fn a_sung_line_is_cut_around_the_syllable_the_dot_is_on() {
        let effect = KaraokeSyllableEffect {
            span: (0.4, 0.6),
            filled: 0.5,
            local: 0.5,
            elapsed: KARAOKE_SYLLABLE_LIFT_SECONDS / 2.0,
            lift: karaoke_syllable_lift(KARAOKE_SYLLABLE_LIFT_SECONDS / 2.0),
            shine: karaoke_syllable_shine(KARAOKE_SYLLABLE_LIFT_SECONDS / 2.0),
        };
        let slices = karaoke_syllable_slices(&effect);
        let plain = |from: f32, to: f32, lift: f32, coloured: bool| KaraokeTextSlice {
            from,
            to,
            shift_x: 0.0,
            lift,
            coloured,
            alpha: 1.0,
        };
        // First the halo: the letters of the syllable all around their place.
        let halo = &slices[..8];
        assert!(halo.iter().all(|slice| {
            (slice.from, slice.to) == (0.4, 0.6)
                && !slice.coloured
                && slice.alpha > 0.0
                && slice.alpha < KARAOKE_SYLLABLE_HALO_ALPHA
                && (slice.shift_x != 0.0 || slice.lift != KARAOKE_SYLLABLE_LIFT)
        }));
        // Then the white text: before, the lifted syllable, after.
        assert_eq!(slices[8], plain(0.0, 0.4, 0.0, false));
        assert_eq!(slices[9], plain(0.4, 0.6, KARAOKE_SYLLABLE_LIFT, false));
        assert_eq!(slices[10], plain(0.6, 1.0, 0.0, false));
        // Sung text, in the character colour, up to the dot.
        assert_eq!(slices[11], plain(0.0, 0.4, 0.0, true));
        assert_eq!(slices[12], plain(0.4, 0.5, KARAOKE_SYLLABLE_LIFT, true));
        // The sung letters of the syllable whitened.
        assert_eq!(slices.len(), 14);
        assert!(slices[13].alpha > 0.0 && slices[13].alpha < KARAOKE_SYLLABLE_SHINE_ALPHA);
        // Once the shine is over and the dot on the first syllable, nothing
        // empty is drawn.
        let settled = KaraokeSyllableEffect {
            span: (0.0, 0.3),
            filled: 0.0,
            lift: 0.0,
            shine: 0.0,
            ..effect
        };
        assert_eq!(
            karaoke_syllable_slices(&settled),
            vec![plain(0.0, 0.3, 0.0, false), plain(0.3, 1.0, 0.0, false)]
        );
    }

    #[test]
    fn a_long_karaoke_label_is_shrunk_to_stay_on_the_band() {
        let fit = |x: f32, width: f32| BadgeFit {
            hidden: false,
            rect: rect(x, 10.0, width, 40.0),
            scale: 1.0,
        };
        // Enough room: nothing changes.
        let free = fit(100.0, 198.0);
        assert_eq!(keep_badge_on_band(free, 300.0, 2.0, 4.0), free);
        // 400 px of name for 196 px of room: shrunk against its line.
        let shrunk = keep_badge_on_band(fit(-102.0, 400.0), 300.0, 2.0, 102.0);
        assert!((shrunk.scale - 0.49).abs() < 1e-4, "{shrunk:?}");
        assert!((shrunk.rect.x - 102.0).abs() < 1e-3);
        assert!((shrunk.rect.x + shrunk.rect.width - 298.0).abs() < 1e-3);
        assert!((shrunk.rect.height - 40.0 * 0.49).abs() < 1e-3);
        // No room at all: smallest size, held at the edge of the band.
        let held = keep_badge_on_band(fit(-398.0, 400.0), 4.0, 2.0, 4.0);
        assert_eq!(held.scale, BADGE_EDGE_MIN_SCALE);
        assert_eq!(held.rect.x, 4.0);
        // A hidden label stays hidden.
        let hidden = BadgeFit {
            hidden: true,
            ..fit(-102.0, 400.0)
        };
        assert_eq!(keep_badge_on_band(hidden, 300.0, 2.0, 4.0), hidden);
    }

    #[test]
    fn free_badge_keeps_its_place_and_size() {
        let badge = rect(0.0, 10.0, 100.0, 40.0);
        let fit = fit_against(badge, 110.0, &[(rect(300.0, 10.0, 50.0, 40.0), false)]);
        assert_eq!(
            fit,
            BadgeFit {
                hidden: false,
                rect: badge,
                scale: 1.0
            }
        );
    }

    #[test]
    fn badge_over_same_character_is_hidden() {
        let badge = rect(0.0, 10.0, 100.0, 40.0);
        let fit = fit_against(badge, 110.0, &[(rect(50.0, 10.0, 20.0, 40.0), true)]);
        assert!(fit.hidden);
    }

    #[test]
    fn badge_over_other_character_shrinks_against_its_line_at_full_opacity() {
        let badge = rect(0.0, 10.0, 100.0, 40.0);
        let line_x = 110.0;
        // Another character's line ends 40 px before this line.
        let other = rect(-200.0, 10.0, 270.0, 40.0);
        let fit = fit_against(badge, line_x, &[(other, false)]);
        assert!(!fit.hidden);
        assert!(fit.scale < 1.0 && fit.scale >= 0.05);
        assert!(!rects_overlap(&fit.rect, &other));
        assert!((fit.rect.x + fit.rect.width - (line_x - 2.0)).abs() < 1e-3);
        assert!((fit.rect.height - 40.0 * fit.scale).abs() < 1e-3);
        // One step larger would still collide: the label is as big as it can be.
        let larger = fit.scale + BADGE_FIT_STEP;
        let larger_rect = rect(
            line_x - 2.0 - 100.0 * larger,
            10.0,
            100.0 * larger,
            40.0 * larger,
        );
        assert!(rects_overlap(&larger_rect, &other));
    }

    #[test]
    fn badge_never_shrinks_below_five_percent() {
        let badge = rect(0.0, 10.0, 100.0, 40.0);
        let fit = fit_against(badge, 110.0, &[(rect(-500.0, 0.0, 1000.0, 100.0), false)]);
        assert!(!fit.hidden);
        assert!((fit.scale - 0.05).abs() < 1e-4);
    }

    #[test]
    fn export_fit_ignores_its_own_line_and_tells_characters_apart() {
        let badge = rect(0.0, 10.0, 100.0, 40.0);
        let line_x = 110.0;
        let mut lines = std::collections::HashMap::new();
        lines.insert(1, (rect(line_x, 10.0, 200.0, 40.0), "Alice".to_string()));
        lines.insert(2, (rect(-200.0, 10.0, 270.0, 40.0), "Bob".to_string()));
        let fit = fit_character_badge_among_lines(badge, line_x, 2.0, 1, "Alice", &lines);
        assert!(!fit.hidden);
        assert!(fit.scale < 1.0);
        assert_eq!(
            fit,
            fit_against(badge, line_x, &[(rect(-200.0, 10.0, 270.0, 40.0), false)])
        );

        lines.insert(3, (rect(20.0, 10.0, 10.0, 40.0), "Alice".to_string()));
        let fit = fit_character_badge_among_lines(badge, line_x, 2.0, 1, "Alice", &lines);
        assert!(fit.hidden);
    }

    #[test]
    fn rgba8_rounds_channels() {
        assert_eq!(rgba8(LOOP_MARKER_COLOR), [217, 38, 38, 230]);
        assert_eq!(
            [
                rgba8(LOOP_MARKER_COLOR)[0],
                rgba8(LOOP_MARKER_COLOR)[1],
                rgba8(LOOP_MARKER_COLOR)[2]
            ],
            LOOP_NUMBER_COLOR
        );
    }

    #[test]
    fn breath_arrow_head_ends_at_the_tip() {
        let body = rect(10.0, 20.0, 100.0, 40.0);
        let [shaft, head_a, head_b] = breath_arrow_bars(body, true, 4.0, 8.0);
        let [_, shaft_end] = rotated_bar_ends(shaft.0, shaft.1, shaft.2, shaft.3);
        assert!((shaft_end.0 - 106.0).abs() < 1e-3);
        assert!((shaft_end.1 - 24.0).abs() < 1e-3);
        assert_eq!((head_a.0, head_a.1), (106.0, 24.0));
        assert_eq!((head_b.0, head_b.1), (106.0, 24.0));
    }
}
