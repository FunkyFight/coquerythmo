//! Procedural bubble outlines for the Comic Dubs studio tools.
//!
//! Every generator returns a normalized polygon (page coordinates in 0..=1)
//! that respects the document limits: at most 128 vertices, inside the page.

use crate::comic_dubs::Point;

/// Shapes drawn by dragging a box on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    Ellipse,
    Rectangle,
    Shout,
    Thought,
    Narration,
}

impl ShapeKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ellipse => "Bulle ronde",
            Self::Rectangle => "Bulle rectangulaire",
            Self::Shout => "Bulle de cri",
            Self::Thought => "Bulle de pensée",
            Self::Narration => "Cartouche de narration",
        }
    }
}

/// Builds the outline fitting the box spanned by `a` and `b`. Returns `None`
/// when the box is too small to be a deliberate drag.
pub fn shape_points(kind: ShapeKind, a: Point, b: Point) -> Option<Vec<Point>> {
    let (x0, x1) = (a.x.min(b.x).clamp(0.0, 1.0), a.x.max(b.x).clamp(0.0, 1.0));
    let (y0, y1) = (a.y.min(b.y).clamp(0.0, 1.0), a.y.max(b.y).clamp(0.0, 1.0));
    if x1 - x0 < 0.01 || y1 - y0 < 0.01 {
        return None;
    }
    let bounds = Bounds { x0, y0, x1, y1 };
    let points = match kind {
        ShapeKind::Ellipse => radial(bounds, 48, |_| 1.0),
        ShapeKind::Rectangle => rounded_rect(bounds, 0.18),
        ShapeKind::Narration => vec![
            Point { x: x0, y: y0 },
            Point { x: x1, y: y0 },
            Point { x: x1, y: y1 },
            Point { x: x0, y: y1 },
        ],
        ShapeKind::Shout => radial(bounds, 36, |index| {
            // Irregular spikes read as a shout; the fixed pattern keeps
            // the shape deterministic.
            const SPIKES: [f32; 6] = [1.0, 0.93, 1.0, 0.88, 0.97, 0.91];
            if index % 2 == 0 {
                SPIKES[(index / 2) % SPIKES.len()]
            } else {
                0.68
            }
        }),
        ShapeKind::Thought => radial(bounds, 96, |index| {
            // Ten scalloped bumps with sharp valleys.
            let phase = index as f32 / 96.0 * std::f32::consts::TAU * 5.0;
            0.86 + 0.14 * phase.sin().abs()
        }),
    };
    Some(clamped(points))
}

#[derive(Clone, Copy)]
struct Bounds {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

fn radial(bounds: Bounds, count: usize, radius: impl Fn(usize) -> f32) -> Vec<Point> {
    let cx = (bounds.x0 + bounds.x1) * 0.5;
    let cy = (bounds.y0 + bounds.y1) * 0.5;
    let rx = (bounds.x1 - bounds.x0) * 0.5;
    let ry = (bounds.y1 - bounds.y0) * 0.5;
    (0..count)
        .map(|index| {
            let angle =
                index as f32 / count as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let scale = radius(index);
            Point {
                x: cx + angle.cos() * rx * scale,
                y: cy + angle.sin() * ry * scale,
            }
        })
        .collect()
}

fn rounded_rect(bounds: Bounds, radius_fraction: f32) -> Vec<Point> {
    let width = bounds.x1 - bounds.x0;
    let height = bounds.y1 - bounds.y0;
    let radius = width.min(height) * radius_fraction;
    let corners = [
        (bounds.x1 - radius, bounds.y0 + radius, -90.0_f32),
        (bounds.x1 - radius, bounds.y1 - radius, 0.0),
        (bounds.x0 + radius, bounds.y1 - radius, 90.0),
        (bounds.x0 + radius, bounds.y0 + radius, 180.0),
    ];
    corners
        .iter()
        .flat_map(|(cx, cy, start)| {
            (0..=6).map(move |step| {
                let angle = (start + step as f32 * 15.0).to_radians();
                Point {
                    x: cx + angle.cos() * radius,
                    y: cy + angle.sin() * radius,
                }
            })
        })
        .collect()
}

fn clamped(points: Vec<Point>) -> Vec<Point> {
    points
        .into_iter()
        .map(|point| Point {
            x: point.x.clamp(0.0, 1.0),
            y: point.y.clamp(0.0, 1.0),
        })
        .collect()
}

fn centroid(points: &[Point]) -> Point {
    let count = points.len().max(1) as f32;
    Point {
        x: points.iter().map(|point| point.x).sum::<f32>() / count,
        y: points.iter().map(|point| point.y).sum::<f32>() / count,
    }
}

/// Adds a speech tail pointing down and slightly left from the lowest part
/// of the outline. The tip is a regular vertex the user can then drag.
pub fn with_tail(points: &[Point]) -> Vec<Point> {
    let count = points.len();
    if count < 3 {
        return points.to_vec();
    }
    let center = centroid(points);
    let min_y = points.iter().map(|point| point.y).fold(1.0, f32::min);
    let max_y = points.iter().map(|point| point.y).fold(0.0, f32::max);
    let min_x = points.iter().map(|point| point.x).fold(1.0, f32::min);
    let max_x = points.iter().map(|point| point.x).fold(0.0, f32::max);
    let (width, height) = (max_x - min_x, max_y - min_y);
    let score = |point: Point| (point.y - center.y) - 0.35 * (point.x - center.x).abs();
    let tip_from = |base: Point| Point {
        x: (base.x - width * 0.16).clamp(0.0, 1.0),
        y: (base.y + height * 0.55).clamp(0.0, 1.0),
    };
    if count >= 12 {
        let half = (count / 16).max(1);
        if count - (2 * half - 1) + 1 > 128 {
            return points.to_vec();
        }
        let lowest = (0..count)
            .max_by(|a, b| score(points[*a]).total_cmp(&score(points[*b])))
            .unwrap_or(0);
        let mut result = Vec::with_capacity(count);
        for offset in 0..count {
            let index = (lowest + half + offset) % count;
            result.push(points[index]);
            if offset == count - 2 * half {
                result.push(tip_from(points[lowest]));
                break;
            }
        }
        return result;
    }
    if count + 3 > 128 {
        return points.to_vec();
    }
    let edge = (0..count)
        .max_by(|a, b| {
            let mid = |index: usize| {
                let (p, q) = (points[index], points[(index + 1) % count]);
                Point {
                    x: (p.x + q.x) * 0.5,
                    y: (p.y + q.y) * 0.5,
                }
            };
            score(mid(*a)).total_cmp(&score(mid(*b)))
        })
        .unwrap_or(0);
    let (p, q) = (points[edge], points[(edge + 1) % count]);
    let along = |ratio: f32| Point {
        x: p.x + (q.x - p.x) * ratio,
        y: p.y + (q.y - p.y) * ratio,
    };
    let mut result = points.to_vec();
    let middle = along(0.5);
    result.splice(
        edge + 1..edge + 1,
        [along(0.38), tip_from(middle), along(0.62)],
    );
    result
}

/// One Chaikin corner-cutting pass. Leaves the outline untouched when the
/// result would exceed the 128-vertex limit.
pub fn smoothed(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 || points.len() * 2 > 128 {
        return points.to_vec();
    }
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .flat_map(|(p, q)| {
            [
                Point {
                    x: p.x * 0.75 + q.x * 0.25,
                    y: p.y * 0.75 + q.y * 0.25,
                },
                Point {
                    x: p.x * 0.25 + q.x * 0.75,
                    y: p.y * 0.25 + q.y * 0.75,
                },
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(points: &[Point]) -> f32 {
        points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
            .map(|(a, b)| a.x * b.y - b.x * a.y)
            .sum::<f32>()
            .abs()
            * 0.5
    }

    #[test]
    fn every_tool_shape_is_a_valid_document_polygon() {
        let a = Point { x: 0.2, y: 0.1 };
        let b = Point { x: 0.6, y: 0.4 };
        for kind in [
            ShapeKind::Ellipse,
            ShapeKind::Rectangle,
            ShapeKind::Shout,
            ShapeKind::Thought,
            ShapeKind::Narration,
        ] {
            let points = shape_points(kind, a, b).unwrap();
            assert!(points.len() >= 3 && points.len() <= 128, "{kind:?}");
            assert!(points
                .iter()
                .all(|point| (0.0..=1.0).contains(&point.x) && (0.0..=1.0).contains(&point.y)));
            assert!(area(&points) > 0.01, "{kind:?}");
        }
        assert!(shape_points(ShapeKind::Ellipse, a, a).is_none());
    }

    #[test]
    fn tails_point_below_the_bubble_and_keep_the_vertex_budget() {
        let ellipse = shape_points(
            ShapeKind::Ellipse,
            Point { x: 0.3, y: 0.2 },
            Point { x: 0.7, y: 0.4 },
        )
        .unwrap();
        let tailed = with_tail(&ellipse);
        assert!(tailed.len() <= 128);
        assert!(tailed.iter().any(|point| point.y > 0.5));
        // The same operation yields the same vertex count on another pose.
        let moved = ellipse
            .iter()
            .map(|point| Point {
                x: point.x + 0.05,
                y: point.y,
            })
            .collect::<Vec<_>>();
        assert_eq!(with_tail(&moved).len(), tailed.len());

        let triangle = [
            Point { x: 0.2, y: 0.2 },
            Point { x: 0.6, y: 0.2 },
            Point { x: 0.4, y: 0.4 },
        ];
        let tailed = with_tail(&triangle);
        assert_eq!(tailed.len(), 6);
        assert!(tailed.iter().any(|point| point.y > 0.41));
    }

    #[test]
    fn smoothing_doubles_vertices_until_the_limit() {
        let square = [
            Point { x: 0.1, y: 0.1 },
            Point { x: 0.5, y: 0.1 },
            Point { x: 0.5, y: 0.5 },
            Point { x: 0.1, y: 0.5 },
        ];
        let smooth = smoothed(&square);
        assert_eq!(smooth.len(), 8);
        assert!(area(&smooth) < area(&square));
        let dense = vec![Point { x: 0.5, y: 0.5 }; 70];
        assert_eq!(smoothed(&dense).len(), 70);
    }
}
