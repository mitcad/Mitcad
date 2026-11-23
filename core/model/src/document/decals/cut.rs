// SPDX-License-Identifier: MIT
//! Decals cut at their face's edges (mitcad#2266): the pieces of a decal's
//! grid that lie on its face, as triangles in the face's coordinates.
//!
//! The face's boundary (its edges, outer and inner loops alike) is taken
//! into the face's coordinates near the decal: a plane's as they are, a
//! cylindrical or conical face's development unrolled within half a turn
//! of the decal's middle. A point of the decal lies on the face when a ray
//! from it crosses the boundary an odd number of times; the ray runs along
//! the face's `y` (on a cone away from its apex's image), so that it never
//! meets the place where the unrolled face is cut open, and a seam (an
//! edge along the ray, the face on both sides) never counts. An edge that
//! crosses that place is continued past it from both of its ends, so that
//! the boundary is whole all across the decal.
//!
//! Each piece of the grid is cut on its own: a piece that no edge comes
//! near is whole or off the face; another is split into strips at its
//! corners, the edges' ends and their crossings, and each strip into cells
//! between the lines that cross it (no line crosses a cell); the cells on
//! the face are kept, two triangles each.

use std::f64::consts::PI;

/// How the face's coordinates near a decal unroll.
#[derive(Debug, Clone, Copy)]
pub(super) enum Unrolled {
    /// A plane: its coordinates as they are.
    Flat,
    /// A cylinder: `x` around it, once around every `period` mm.
    Around { period: f64 },
    /// A cone's development: a fan of `angle` radians about its apex's
    /// image `apex`.
    Fan { apex: [f64; 2], angle: f64 },
}

/// A piece of the boundary in the face's coordinates (`a` to `b`) and the
/// same ends unrolled about the decal's middle: `[across, along]` (a fan's
/// angle from the middle and distance from the apex; otherwise `x` from the
/// middle and `y`).
#[derive(Debug, Clone, Copy)]
struct Edge {
    a: [f64; 2],
    b: [f64; 2],
    across: [f64; 2],
}

/// A face's boundary near a decal, ready for its cells.
pub(super) struct Boundary {
    unrolled: Unrolled,
    middle: [f64; 2],
    edges: Vec<Edge>,
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

impl Unrolled {
    /// The coordinate around the face of `q` from `middle`: a fan's angle,
    /// otherwise `x`.
    fn across(&self, middle: [f64; 2], q: [f64; 2]) -> f64 {
        match self {
            Self::Flat | Self::Around { .. } => q[0] - middle[0],
            Self::Fan { apex, .. } => {
                let (m, p) = (sub(middle, *apex), sub(q, *apex));
                cross(m, p).atan2(dot(m, p))
            }
        }
    }

    /// `q`'s coordinate around the face counted on from `p` (at `from`),
    /// past the place where the face is cut open if need be.
    fn across_after(&self, from: f64, p: [f64; 2], q: [f64; 2], middle: [f64; 2]) -> f64 {
        match self {
            Self::Flat | Self::Around { .. } => self.across(middle, q),
            Self::Fan { apex, .. } => {
                let (a, b) = (sub(p, *apex), sub(q, *apex));
                from + cross(a, b).atan2(dot(a, b))
            }
        }
    }

    /// Half the face once around: two points of an edge farther apart
    /// around it lie on both sides of the place where it is cut open.
    fn half_turn(&self) -> f64 {
        match self {
            Self::Flat => f64::INFINITY,
            Self::Around { period } => period / 2.0,
            Self::Fan { angle, .. } => (angle / 2.0).min(PI),
        }
    }
}

impl Boundary {
    /// The boundary's edges (each a run of points of the face in the
    /// design) in the face's coordinates near `middle` (`to_face` takes a
    /// design point to the face's coordinates on the turn nearest to a
    /// point of them; none at an axis or an apex, where a run is broken).
    pub(super) fn new(
        unrolled: Unrolled,
        middle: [f64; 2],
        runs: &[Vec<[f64; 3]>],
        to_face: &dyn Fn([f64; 3], [f64; 2]) -> Option<[f64; 2]>,
    ) -> Self {
        let mut edges = Vec::new();
        let half = unrolled.half_turn();
        for run in runs {
            let mut previous: Option<([f64; 3], [f64; 2], f64)> = None;
            for &p in run {
                let Some(q) = to_face(p, middle) else {
                    previous = None;
                    continue;
                };
                let around = unrolled.across(middle, q);
                if let Some((p0, q0, around0)) = previous {
                    if (around - around0).abs() <= half {
                        edges.push(Edge {
                            a: q0,
                            b: q,
                            across: [around0, around],
                        });
                    } else {
                        // Across the place where the face is cut open:
                        // continued past it from each end.
                        if let Some(q1) = to_face(p, q0) {
                            edges.push(Edge {
                                a: q0,
                                b: q1,
                                across: [around0, unrolled.across_after(around0, q0, q1, middle)],
                            });
                        }
                        if let Some(q2) = to_face(p0, q) {
                            edges.push(Edge {
                                a: q2,
                                b: q,
                                across: [unrolled.across_after(around, q, q2, middle), around],
                            });
                        }
                    }
                }
                previous = Some((p, q, around));
            }
        }
        Self {
            unrolled,
            middle,
            edges,
        }
    }

    /// Whether the face gave no boundary to cut at.
    pub(super) fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Whether `q` (face coordinates near the decal) lies on the face: the
    /// ray from it along `y` (away from a fan's apex) crosses the boundary
    /// an odd number of times.
    fn holds(&self, q: [f64; 2]) -> bool {
        let at = self.unrolled.across(self.middle, q);
        let mut odd = false;
        for e in &self.edges {
            if (e.across[0] > at) == (e.across[1] > at) {
                continue;
            }
            let beyond = match self.unrolled {
                Unrolled::Flat | Unrolled::Around { .. } => {
                    let t = (q[0] - e.a[0]) / (e.b[0] - e.a[0]);
                    let t = if t.is_finite() {
                        t.clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    lerp(e.a, e.b, t)[1] > q[1]
                }
                Unrolled::Fan { apex, .. } => {
                    let out = sub(q, apex);
                    let reach = out[0].hypot(out[1]);
                    if reach <= 0.0 {
                        continue;
                    }
                    let dir = [out[0] / reach, out[1] / reach];
                    let (ca, cb) = (cross(dir, sub(e.a, apex)), cross(dir, sub(e.b, apex)));
                    let t = ca / (ca - cb);
                    let t = if t.is_finite() {
                        t.clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    dot(sub(lerp(e.a, e.b, t), apex), dir) > reach
                }
            };
            if beyond {
                odd = !odd;
            }
        }
        odd
    }

    /// The edges that come within the box `low`..`high`.
    fn near(&self, low: [f64; 2], high: [f64; 2]) -> Vec<([f64; 2], [f64; 2])> {
        self.edges
            .iter()
            .filter(|e| {
                (0..2).all(|i| e.a[i].min(e.b[i]) <= high[i] && e.a[i].max(e.b[i]) >= low[i])
            })
            .map(|e| (e.a, e.b))
            .collect()
    }
}

/// What a piece of a decal's grid is on its face.
pub(super) enum PieceCut {
    /// All of it.
    Whole,
    /// None of it.
    Off,
    /// These triangles (face coordinates, counter-clockwise).
    Triangles(Vec<[[f64; 2]; 3]>),
}

/// The part on the face of a piece of the grid, a parallelogram whose
/// corners are `corners` (face coordinates, in order around it); `size` is
/// the decal's size for tolerances.
pub(super) fn cut_piece(boundary: &Boundary, corners: [[f64; 2]; 4], size: f64) -> PieceCut {
    let low = [0, 1].map(|i| corners.iter().map(|c| c[i]).fold(f64::INFINITY, f64::min));
    let high = [0, 1].map(|i| {
        corners
            .iter()
            .map(|c| c[i])
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let near = boundary.near(low, high);
    let middle = [
        corners.iter().map(|c| c[0]).sum::<f64>() / 4.0,
        corners.iter().map(|c| c[1]).sum::<f64>() / 4.0,
    ];
    if near.is_empty() {
        return if boundary.holds(middle) {
            PieceCut::Whole
        } else {
            PieceCut::Off
        };
    }
    let eps = 1e-9 * size.max(1.0);
    // Inside the piece: on the inner side of each of its sides.
    let turn = cross(sub(corners[1], corners[0]), sub(corners[2], corners[1])).signum();
    let in_piece = |q: [f64; 2]| {
        (0..4).all(|k| {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            cross(sub(b, a), sub(q, a)) * turn > 0.0
        })
    };
    let mut lines: Vec<([f64; 2], [f64; 2])> =
        (0..4).map(|k| (corners[k], corners[(k + 1) % 4])).collect();
    lines.extend(near);
    // The strips' sides: the corners, the lines' ends and crossings.
    let mut xs: Vec<f64> = lines.iter().flat_map(|(a, b)| [a[0], b[0]]).collect();
    for (i, &first) in lines.iter().enumerate() {
        for &second in &lines[i + 1..] {
            if let Some(x) = crossing(first, second) {
                xs.push(x);
            }
        }
    }
    xs.retain(|x| *x >= low[0] - eps && *x <= high[0] + eps);
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() <= eps);
    let mut triangles = Vec::new();
    for pair in xs.windows(2) {
        let (x0, x1) = (pair[0], pair[1]);
        if x1 - x0 <= eps {
            continue;
        }
        let xm = 0.5 * (x0 + x1);
        // The lines across the strip, bottom to top: (y at x0, x1, middle).
        let mut across: Vec<[f64; 3]> = lines
            .iter()
            .filter(|(a, b)| {
                (b[0] - a[0]).abs() > eps
                    && a[0].min(b[0]) <= x0 + eps
                    && a[0].max(b[0]) >= x1 - eps
            })
            .map(|(a, b)| {
                let y = |x: f64| a[1] + (b[1] - a[1]) * (x - a[0]) / (b[0] - a[0]);
                [y(x0), y(x1), y(xm)]
            })
            .collect();
        across.sort_by(|p, q| p[2].total_cmp(&q[2]));
        for cell in across.windows(2) {
            let (below, above) = (cell[0], cell[1]);
            if above[2] - below[2] <= eps {
                continue;
            }
            let centre = [xm, 0.5 * (below[2] + above[2])];
            if !in_piece(centre) || !boundary.holds(centre) {
                continue;
            }
            let a = [x0, below[0]];
            let b = [x1, below[1]];
            let c = [x1, above[1]];
            let d = [x0, above[0]];
            if c[1] - b[1] > eps {
                triangles.push([a, b, c]);
            }
            if d[1] - a[1] > eps {
                triangles.push([a, c, d]);
            }
        }
    }
    PieceCut::Triangles(triangles)
}

/// The `x` where two segments cross, if they do (ends included).
fn crossing((a, b): ([f64; 2], [f64; 2]), (c, d): ([f64; 2], [f64; 2])) -> Option<f64> {
    let r = sub(b, a);
    let s = sub(d, c);
    let denominator = cross(r, s);
    if denominator.abs() < 1e-300 {
        return None;
    }
    let ca = sub(c, a);
    let t = cross(ca, s) / denominator;
    let u = cross(ca, r) / denominator;
    if (-1e-12..=1.0 + 1e-12).contains(&t) && (-1e-12..=1.0 + 1e-12).contains(&u) {
        Some(a[0] + r[0] * t)
    } else {
        None
    }
}

/// The area of a triangle in the face's coordinates, positive when it runs
/// counter-clockwise.
pub(super) fn area([a, b, c]: [[f64; 2]; 3]) -> f64 {
    0.5 * cross(sub(b, a), sub(c, a))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(low: f64, high: f64) -> Vec<[f64; 3]> {
        vec![
            [low, low, 0.0],
            [high, low, 0.0],
            [high, high, 0.0],
            [low, high, 0.0],
            [low, low, 0.0],
        ]
    }

    fn flat(runs: &[Vec<[f64; 3]>]) -> Boundary {
        Boundary::new(Unrolled::Flat, [0.0, 0.0], runs, &|p, _| Some([p[0], p[1]]))
    }

    fn cut_area(boundary: &Boundary, corners: [[f64; 2]; 4]) -> f64 {
        match cut_piece(boundary, corners, 10.0) {
            PieceCut::Whole => area([corners[0], corners[1], corners[2]]).abs() * 2.0,
            PieceCut::Off => 0.0,
            PieceCut::Triangles(triangles) => {
                for t in &triangles {
                    assert!(area(*t) > 0.0, "{t:?} runs clockwise");
                }
                triangles.iter().map(|t| area(*t)).sum()
            }
        }
    }

    #[test]
    fn a_piece_is_cut_at_the_outer_and_inner_loops() {
        // A 20 x 20 face with a 4 x 4 hole in its middle.
        let face = flat(&[square(-10.0, 10.0), square(-2.0, 2.0)]);
        let inside = [[-8.0, -8.0], [-4.0, -8.0], [-4.0, -4.0], [-8.0, -4.0]];
        assert!(matches!(cut_piece(&face, inside, 10.0), PieceCut::Whole));
        let off = [[12.0, 0.0], [14.0, 0.0], [14.0, 2.0], [12.0, 2.0]];
        assert!(matches!(cut_piece(&face, off, 10.0), PieceCut::Off));
        // Half past the right edge: 5 of 10 across.
        let across = [[5.0, 0.0], [15.0, 0.0], [15.0, 5.0], [5.0, 5.0]];
        assert!((cut_area(&face, across) - 25.0).abs() < 1e-9);
        // Over the hole: 6 x 6 less the hole's 16.
        let over = [[-3.0, -3.0], [3.0, -3.0], [3.0, 3.0], [-3.0, 3.0]];
        assert!((cut_area(&face, over) - 20.0).abs() < 1e-9);
        // Turned 45 degrees over the corner (10, 10): a quarter of it.
        let r = 2.0f64.sqrt() * 2.0;
        let turned = [
            [10.0, 10.0 - r],
            [10.0 + r, 10.0],
            [10.0, 10.0 + r],
            [10.0 - r, 10.0],
        ];
        assert!((cut_area(&face, turned) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_seam_does_not_cut_a_cylinder_and_its_ends_do() {
        // A cylinder unrolled 2 * pi * 10 around, its ends at y = 0 and 30,
        // its seam at x = 0; the circles given as points every 30 degrees,
        // their x taken within half a turn of the middle.
        let period = 20.0 * PI;
        let middle = [0.0, 0.0];
        let to_face = move |p: [f64; 3], near: [f64; 2]| {
            let mut x = 10.0 * p[1].atan2(p[0]);
            x += period * ((near[0] - x) / period).round();
            Some([x, p[2]])
        };
        let circle = |z: f64| -> Vec<[f64; 3]> {
            (0..=12)
                .map(|k| {
                    let a = PI / 6.0 * k as f64;
                    [10.0 * a.cos(), 10.0 * a.sin(), z]
                })
                .collect()
        };
        let seam = vec![[10.0, 0.0, 0.0], [10.0, 0.0, 30.0]];
        let side = Boundary::new(
            Unrolled::Around { period },
            middle,
            &[circle(0.0), circle(30.0), seam],
            &to_face,
        );
        // Across the seam, wholly on the side.
        let piece = [[-5.0, 10.0], [5.0, 10.0], [5.0, 20.0], [-5.0, 20.0]];
        assert!((cut_area(&side, piece) - 100.0).abs() < 1e-9);
        // Past the top end by 5.
        let top = [[-5.0, 25.0], [5.0, 25.0], [5.0, 35.0], [-5.0, 35.0]];
        assert!((cut_area(&side, top) - 50.0).abs() < 1e-9);
        // Half a turn away, where the unrolled face is cut open.
        let back = [
            [period / 2.0 - 5.0, 10.0],
            [period / 2.0 + 5.0, 10.0],
            [period / 2.0 + 5.0, 20.0],
            [period / 2.0 - 5.0, 20.0],
        ];
        let behind = Boundary::new(
            Unrolled::Around { period },
            [period / 2.0, 15.0],
            &[circle(0.0), circle(30.0)],
            &to_face,
        );
        assert!((cut_area(&behind, back) - 100.0).abs() < 1e-9);
    }
}
