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
