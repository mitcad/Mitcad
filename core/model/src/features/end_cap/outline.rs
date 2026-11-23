// SPDX-License-Identifier: MIT
//! A closed section's outer loop (mitcad#2270, mitcad#2287;
//! `commands.md`, "End caps", "The plate"). The member's end face's
//! boundary is chained into loops in the end frame's coordinates (x and y
//! across the member); the loop that encloses the most is the outer one,
//! the others the face's holes. The section is closed when
//!
//! - the outer loop is convex (a tube's or a bar's outline: square,
//!   rectangular, round, oval, elliptical, hexagonal, octagonal, or a
//!   convex section sketched by the user), whatever holes the face has; or
//! - the face has one hole and it is the inside of a wall round it (a
//!   hollow section of any outline: an L-, T- or star-shaped tube, a tube
//!   with a notch): every point of either loop lies between
//!   [`WALL_BAND`] times the wall from the other loop, the wall being the
//!   face's area over the loops' mean length ([`is_wall`]).
//!
//! A solid face whose outline turns inward is an angle's, a channel's or a
//! beam's (a solid L bar is an angle by its geometry), and a face whose
//! holes are no wall's inside a slot profile's (its core bore and
//! chambers): those are open sections, whose plate is the rectangle round
//! them ([`super::Section::new`]).
//!
//! A closed section's plate follows its outer loop: set in by the inset
//! with the sketch offset's construction (`mitcad_sketch::offset::compute`:
//! lines and arcs exactly, an ellipse or a spline as a spline within its
//! tolerance, a corner that turns inward rounded about it), refused when
//! the loop set in falls into parts or vanishes; and its outward corners
//! chamfered: a corner between two straight sides rounded by an arc
//! tangent to both, and every sharp corner (a tangent break) between two
//! curves of any kind.

use std::f64::consts::{PI, TAU};

use mitcad_sketch::geometry::{Curve2, Nurbs};
use mitcad_sketch::offset::{Made, compute, made_curve};

use super::{Corner, Plate, Section, mm};
use crate::kernel::Curve3;
use crate::profile::SegmentGeometry;
use crate::transform::Transform;

type P2 = [f64; 2];

fn dist(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn sub2(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross2(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot2(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn unit2(a: P2) -> Option<P2> {
    let n = a[0].hypot(a[1]);
    (n > 1e-15).then(|| [a[0] / n, a[1] / n])
}

fn along(p: P2, d: P2, s: f64) -> P2 {
    [p[0] + d[0] * s, p[1] + d[1] * s]
}

/// A curve of a loop and whether the loop runs along its parameter.
pub type Piece = (Curve2, bool);

/// A closed loop of curves end to end, counter-clockwise.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub pieces: Vec<Piece>,
}

/// A piece's start and end along the loop (None for a closed curve).
fn ends(piece: &Piece) -> Option<(P2, P2)> {
    let (a, b) = piece.0.ends()?;
    Some(if piece.1 { (a, b) } else { (b, a) })
}

/// A piece's unit tangent along the loop at its start (`at_end` false) or
/// its end.
fn tangent(piece: &Piece, at_end: bool) -> Option<P2> {
    let (lo, hi) = piece.0.domain();
    let t = if at_end == piece.1 { hi } else { lo };
    let d = piece.0.eval(t)[1];
    unit2(if piece.1 { d } else { [-d[0], -d[1]] })
}

/// A boundary curve in the end frame's coordinates (`into`: from the
/// body's coordinates to the frame's), seen along the frame's z: a line,
/// a circle or an arc, an ellipse or an elliptical arc, a spline. A conic
/// whose normal points down (−z) runs the other way round in the frame,
/// so its piece runs against the curve's parameter.
fn planar(curve: &Curve3, into: &Transform) -> Option<Piece> {
    let p2 = |p: [f64; 3]| {
        let q = into.apply_point(p);
        [q[0], q[1]]
    };
    match curve {
        Curve3::Line { start, end } => {
            let (a, b) = (p2(*start), p2(*end));
            (dist(a, b) > 1e-12).then_some((Curve2::Line { a, b }, true))
        }
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => {
            let c = p2(*center);
            let n = into.apply_vector(*normal);
            let x = into.apply_vector(*x_axis);
            let up = n[2] >= 0.0;
            let rotation = x[1].atan2(x[0]);
            let round = (major - minor).abs() <= 1e-9 * major.max(1.0);
            let whole = *closed || end - start >= TAU - 1e-12;
            // Seen from the frame's z, a conic whose normal points down
            // runs clockwise: its parameter is −t of a counter-clockwise
            // one with the same x axis.
            let (u0, u1) = if up { (*start, *end) } else { (-end, -start) };
            let curve = match (round, whole) {
                (true, true) => Curve2::Circle {
                    center: c,
                    radius: *major,
                    start: rotation,
                },
                (true, false) => Curve2::Arc {
                    center: c,
                    radius: *major,
                    start: rotation + u0,
                    end: rotation + u1,
                },
                (false, true) => Curve2::Ellipse {
                    center: c,
                    major: *major,
                    minor: *minor,
                    rotation,
                },
                (false, false) => Curve2::EllipticalArc {
                    center: c,
                    major: *major,
                    minor: *minor,
                    rotation,
                    start: u0,
                    end: u1,
                },
            };
            Some((curve, up))
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => Some((
            Curve2::Nurbs(Nurbs {
                degree: *degree as usize,
                control: poles.iter().map(|p| p2(*p)).collect(),
                weights: weights.clone(),
                knots: knots.clone(),
            }),
            true,
        )),
        Curve3::Point(_) | Curve3::Helix { .. } => None,
    }
}

/// The pieces chained end to end into closed loops (ends within `tol`); a
/// closed curve is a loop of its own, a chain that does not close is left
/// out.
fn loops(pieces: Vec<Piece>, tol: f64) -> Vec<Vec<Piece>> {
    let mut out = Vec::new();
    let mut open = Vec::new();
    for piece in pieces {
        if piece.0.is_closed() {
            out.push(vec![piece]);
        } else {
            open.push(piece);
        }
    }
    let mut used = vec![false; open.len()];
    for first in 0..open.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let Some((start, mut at)) = ends(&open[first]) else {
            continue;
        };
        let mut chain = vec![open[first].clone()];
        loop {
            if chain.len() > 1 && dist(at, start) <= tol {
                out.push(chain);
                break;
            }
            // The nearest unused piece that starts or ends here.
            let next = (0..open.len())
                .filter(|j| !used[*j])
                .filter_map(|j| {
                    let (a, b) = ends(&open[j])?;
                    let (da, db) = (dist(a, at), dist(b, at));
                    let (d, flip) = if da <= db { (da, false) } else { (db, true) };
                    (d <= tol).then_some((d, j, flip))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            let Some((_, j, flip)) = next else {
                break;
            };
            used[j] = true;
            let (curve, forward) = open[j].clone();
            let piece = (curve, forward != flip);
            if let Some((_, end)) = ends(&piece) {
                at = end;
            }
            chain.push(piece);
        }
    }
    out
}

/// Five-point Gauss–Legendre nodes and weights on [−1, 1].
const GAUSS: [(f64, f64); 5] = [
    (0.0, 0.568_888_888_888_888_9),
    (-0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (-0.906_179_845_938_664, 0.236_926_885_056_189_1),
    (0.906_179_845_938_664, 0.236_926_885_056_189_1),
];

/// Half of ∮ x dy − y dx along a curve in its parameter's direction: the
/// signed area its piece adds to a loop. Lines, arcs and ellipses
/// exactly; a spline by Gauss–Legendre quadrature over its knot spans.
fn swept_area(curve: &Curve2) -> f64 {
    let (lo, hi) = curve.domain();
    match curve {
        Curve2::Line { a, b } => 0.5 * cross2(*a, *b),
        Curve2::Circle { radius, .. } => PI * radius * radius,
        Curve2::Ellipse { major, minor, .. } => PI * major * minor,
        Curve2::Arc { center, radius, .. } => {
            let chord = sub2(curve.point(hi), curve.point(lo));
            0.5 * (cross2(*center, chord) + radius * radius * (hi - lo))
        }
        Curve2::EllipticalArc {
            center,
            major,
            minor,
            ..
        } => {
            let chord = sub2(curve.point(hi), curve.point(lo));
            0.5 * (cross2(*center, chord) + major * minor * (hi - lo))
        }
        Curve2::Nurbs(n) => {
            let mut knots: Vec<f64> = n
                .knots
                .iter()
                .copied()
                .filter(|k| *k >= lo && *k <= hi)
                .collect();
            knots.push(lo);
            knots.push(hi);
            knots.sort_by(f64::total_cmp);
            knots.dedup_by(|a, b| (*a - *b).abs() <= 1e-15);
            let mut total = 0.0;
            for w in knots.windows(2) {
                // Each span in eight parts.
                for k in 0..8 {
                    let a = w[0] + (w[1] - w[0]) * f64::from(k) / 8.0;
                    let b = w[0] + (w[1] - w[0]) * f64::from(k + 1) / 8.0;
                    let (mid, half) = (0.5 * (a + b), 0.5 * (b - a));
                    for (x, weight) in GAUSS {
                        let [p, d, _] = n.eval(mid + half * x);
                        total += weight * half * cross2(p, d);
                    }
                }
            }
            0.5 * total
        }
    }
}

/// A loop's signed area: positive counter-clockwise.
fn loop_area(pieces: &[Piece]) -> f64 {
    pieces
        .iter()
        .map(|(c, forward)| {
            let a = swept_area(c);
            if *forward { a } else { -a }
        })
        .sum()
}

/// A loop run the other way round.
fn reversed(pieces: Vec<Piece>) -> Vec<Piece> {
    pieces.into_iter().rev().map(|(c, f)| (c, !f)).collect()
}

/// The pieces of a loop with lines that run on in one line, and arcs of
/// one circle that run on, made one; arcs that make a whole circle a
/// circle (a round tube's outline in two halves).
fn merged(pieces: Vec<Piece>, tol: f64) -> Vec<Piece> {
    fn join(a: &Piece, b: &Piece, tol: f64) -> Option<Piece> {
        match (a, b) {
            ((Curve2::Line { .. }, _), (Curve2::Line { .. }, _)) => {
                let (p0, p1) = ends(a)?;
                let (q0, q1) = ends(b)?;
                let (d1, d2) = (unit2(sub2(p1, p0))?, unit2(sub2(q1, q0))?);
                (dist(p1, q0) <= tol && cross2(d1, d2).abs() <= 1e-9 && dot2(d1, d2) > 0.0)
                    .then_some((Curve2::Line { a: p0, b: q1 }, true))
            }
            (
                (
                    Curve2::Arc {
                        center: c1,
                        radius: r1,
                        start: s1,
                        end: e1,
                    },
                    f1,
                ),
                (
                    Curve2::Arc {
                        center: c2,
                        radius: r2,
                        start: s2,
                        end: e2,
                    },
                    f2,
                ),
            ) if f1 == f2 && dist(*c1, *c2) <= tol && (r1 - r2).abs() <= tol => {
                let (_, p1) = ends(a)?;
                let (q0, _) = ends(b)?;
                if dist(p1, q0) > tol {
                    return None;
                }
                let span = (e1 - s1) + (e2 - s2);
                // The merged arc's parameter starts at the first arc's
                // start when the loop runs along them, else at the
                // second's.
                let from = if *f1 { *s1 } else { *s2 };
                Some(if span >= TAU - 1e-9 {
                    (
                        Curve2::Circle {
                            center: *c1,
                            radius: *r1,
                            start: from,
                        },
                        *f1,
                    )
                } else {
                    (
                        Curve2::Arc {
                            center: *c1,
                            radius: *r1,
                            start: from,
                            end: from + span,
                        },
                        *f1,
                    )
                })
            }
            _ => None,
        }
    }
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        match out.last().and_then(|last| join(last, &piece, tol)) {
            Some(joined) => {
                out.pop();
                out.push(joined);
            }
            None => out.push(piece),
        }
    }
    // Round the seam: the last piece runs on into the first.
    while out.len() > 1 {
        let (last, first) = (&out[out.len() - 1], &out[0]);
        let Some(joined) = join(last, first, tol) else {
            break;
        };
        out.pop();
        out[0] = joined;
    }
    out
}

/// Points round a loop, in order, each piece sampled without its end.
fn polygon(pieces: &[Piece]) -> Vec<P2> {
    let mut points = Vec::new();
    for (curve, forward) in pieces {
        let (lo, hi) = curve.domain();
        let mut samples = curve.sample(lo, hi);
        if !forward {
            samples.reverse();
        }
        samples.pop();
        points.extend(samples);
    }
    points
}

/// Whether a counter-clockwise loop is convex: it never turns right
/// (straight runs and tangent joins turn by nothing).
fn is_convex(pieces: &[Piece], tol: f64) -> bool {
    let mut points: Vec<P2> = Vec::new();
    for p in polygon(pieces) {
        if points.last().is_none_or(|q| dist(*q, p) > tol) {
            points.push(p);
        }
    }
    while points.len() > 1 && dist(points[0], points[points.len() - 1]) <= tol {
        points.pop();
    }
    let n = points.len();
    if n < 3 {
        return false;
    }
    (0..n).all(|i| {
        let (a, b, c) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
        let (e1, e2) = (sub2(b, a), sub2(c, b));
        cross2(e1, e2) >= -1e-6 * e1[0].hypot(e1[1]) * e2[0].hypot(e2[1])
    })
}

/// The box round a loop.
fn bounds(pieces: &[Piece]) -> (P2, P2) {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for (curve, _) in pieces {
        let points = match curve {
            Curve2::Nurbs(_) => {
                let (lo, hi) = curve.domain();
                curve.sample(lo, hi)
            }
            _ => {
                let (lo, hi) = curve.bounds();
                vec![lo, hi]
            }
        };
        for p in points {
            for i in 0..2 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
    }
    (min, max)
}

/// How far, in walls, every point of a hollow section's outer loop may lie
/// from its inner loop and every point of the inner loop from the outer
/// one ([`is_wall`]). A tube's corners lie between: a sharp outer corner
/// √2 walls from a sharp inner one (1.83 from an inner round of a wall),
/// an inner sharp corner 0.59 walls from an outer round of two walls. A
/// slot profile's corners lie three walls or more from its core bore.
pub const WALL_BAND: (f64, f64) = (0.4, 2.5);

/// The distance from a point to a segment.
fn segment_distance(p: P2, a: P2, b: P2) -> f64 {
    let ab = sub2(b, a);
    let len2 = dot2(ab, ab);
    let t = if len2 > 0.0 {
        (dot2(sub2(p, a), ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    dist(p, along(a, ab, t))
}

/// The distance from a point to a closed polygon's edges.
fn polygon_distance(p: P2, polygon: &[P2]) -> f64 {
    let n = polygon.len();
    (0..n)
        .map(|i| segment_distance(p, polygon[i], polygon[(i + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}

/// A closed polygon's length round.
fn perimeter(polygon: &[P2]) -> f64 {
    let n = polygon.len();
    (0..n).map(|i| dist(polygon[i], polygon[(i + 1) % n])).sum()
}

/// A closed polygon with points on its edges at most `step` apart.
fn densified(polygon: &[P2], step: f64) -> Vec<P2> {
    let n = polygon.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        let parts = (dist(a, b) / step).ceil().clamp(1.0, 10_000.0) as usize;
        let d = sub2(b, a);
        for k in 0..parts {
            out.push(along(a, d, k as f64 / parts as f64));
        }
    }
    out
}

/// Whether a face's one hole (`inner`) is the inside of a wall round it
/// (`outer`, the face's outer loop): a hollow section. The wall is the
/// face's area over the mean of the two loops' lengths (exact for a ring
/// between a loop and its offset); every point of each loop lies between
/// [`WALL_BAND`] times the wall from the other loop.
fn is_wall(outer: &[Piece], inner: &[Piece]) -> bool {
    let (po, pi) = (polygon(outer), polygon(inner));
    if po.len() < 3 || pi.len() < 3 {
        return false;
    }
    let ring = loop_area(outer).abs() - loop_area(inner).abs();
    let wall = ring / (0.5 * (perimeter(&po) + perimeter(&pi)));
    if !(wall.is_finite() && wall > 0.0) {
        return false;
    }
    // Points at most half a wall apart (at most 20000 round a loop).
    let step = |p: &[P2]| (0.5 * wall).max(perimeter(p) / 20_000.0);
    let (lo, hi) = (WALL_BAND.0 * wall, WALL_BAND.1 * wall);
    let within = |from: &[P2], to: &[P2]| {
        densified(from, step(from)).into_iter().all(|p| {
            let d = polygon_distance(p, to);
            d >= lo && d <= hi
        })
    };
    within(&pi, &po) && within(&po, &pi)
}

/// The outer loop of an end face's boundary (`curves`, in the body's
/// coordinates; `into` to the end frame's), counter-clockwise in the end
/// frame, when the section is closed (the module's comment): its outer
/// loop convex, or its one hole a wall's inside. None for an open section
/// and for a boundary that makes no loop.
pub fn outer_loop(curves: &[Curve3], into: &Transform) -> Option<Outline> {
    let pieces: Vec<Piece> = curves.iter().filter_map(|c| planar(c, into)).collect();
    if pieces.is_empty() {
        return None;
    }
    let (min, max) = bounds(&pieces);
    let size = (max[0] - min[0]).max(max[1] - min[1]);
    if !size.is_finite() || size <= 0.0 {
        return None;
    }
    let tol = 1e-5 * size.max(1.0);
    let mut all: Vec<(f64, Vec<Piece>)> = loops(pieces, tol)
        .into_iter()
        .map(|l| (loop_area(&l), l))
        .filter(|(area, _)| area.abs() > 1e-9 * size * size)
        .collect();
    let largest = (0..all.len()).max_by(|a, b| all[*a].0.abs().total_cmp(&all[*b].0.abs()))?;
    let (area, pieces) = all.swap_remove(largest);
    let pieces = if area < 0.0 { reversed(pieces) } else { pieces };
    let pieces = snapped(merged(pieces, tol));
    if is_convex(&pieces, tol) {
        return Some(Outline { pieces });
    }
    // A hollow section of any outline: one hole, a wall's inside.
    match all.as_slice() {
        [(_, hole)] if is_wall(&pieces, hole) => Some(Outline { pieces }),
        _ => None,
    }
}

/// A loop whose lines start and end exactly where their neighbours end
/// and start (a curve's own end where a line meets a curve, the middle of
/// the two ends where two lines meet): the kernel's edges meet within its
/// tolerance, the offset wants them to meet exactly.
fn snapped(pieces: Vec<Piece>) -> Vec<Piece> {
    let n = pieces.len();
    if n < 2 {
        return pieces;
    }
    let line = |i: usize| matches!(pieces[i].0, Curve2::Line { .. });
    // Where piece i meets the one after it.
    let joins: Vec<Option<P2>> = (0..n)
        .map(|i| {
            let next = (i + 1) % n;
            let (_, end) = ends(&pieces[i])?;
            let (start, _) = ends(&pieces[next])?;
            match (line(i), line(next)) {
                (true, true) => Some([0.5 * (end[0] + start[0]), 0.5 * (end[1] + start[1])]),
                (true, false) => Some(start),
                (false, true) => Some(end),
                (false, false) => None,
            }
        })
        .collect();
    pieces
        .iter()
        .enumerate()
        .map(|(i, piece)| match (ends(piece), line(i)) {
            (Some((a, b)), true) => (
                Curve2::Line {
                    a: joins[(i + n - 1) % n].unwrap_or(a),
                    b: joins[i].unwrap_or(b),
                },
                true,
            ),
            _ => piece.clone(),
        })
        .collect()
}

impl Outline {
    /// Its area, mm².
    pub fn area(&self) -> f64 {
        loop_area(&self.pieces).abs()
    }

    /// The box round it.
    pub fn bounds(&self) -> (P2, P2) {
        bounds(&self.pieces)
    }

    /// Whether it is one circle.
    pub fn is_circle(&self) -> bool {
        matches!(self.pieces.as_slice(), [(Curve2::Circle { .. }, _)])
    }

    /// The loop as profile geometry, in order round it: lines from their
    /// start to their end along the loop, arcs and elliptical arcs
    /// counter-clockwise, a spline turned round where the loop runs
    /// against it.
    pub fn segments(&self) -> Vec<SegmentGeometry> {
        self.pieces
            .iter()
            .map(|piece| match piece {
                (Curve2::Line { .. }, _) => {
                    let (start, end) = ends(piece).expect("a line has ends");
                    SegmentGeometry::Line { start, end }
                }
                (Curve2::Nurbs(n), false) => Curve2::Nurbs(reversed_nurbs(n)).whole(),
                (curve, _) => curve.whole(),
            })
            .collect()
    }

    /// Points round it, counter-clockwise (a preview's polygon).
    pub fn points(&self) -> Vec<P2> {
        polygon(&self.pieces)
    }
}

/// A spline run the other way.
fn reversed_nurbs(n: &Nurbs) -> Nurbs {
    let (lo, hi) = n.domain();
    Nurbs {
        degree: n.degree,
        control: n.control.iter().rev().copied().collect(),
        weights: n.weights.iter().rev().copied().collect(),
        knots: n.knots.iter().rev().map(|k| lo + hi - k).collect(),
    }
}

/// Why a loop cannot be set in.
#[derive(Debug, Clone, PartialEq)]
enum SetInError {
    /// Nothing, or no closed loop, is left.
    Vanishes(String),
    /// It falls into parts (across a waist narrower than twice the inset).
    Splits(String),
}

/// The loop set in by `inset` (the sketch offset to its inside, the left
/// of a counter-clockwise loop): curves that vanish at that distance left
/// out, the rest meeting where they cross, a corner that turns inward
/// rounded about it. A loop that falls into parts, or loses a part, is
/// refused.
fn set_in(outline: &Outline, inset: f64) -> Result<Outline, SetInError> {
    if inset <= 1e-9 {
        return Ok(outline.clone());
    }
    let name = |i: usize| format!("curve {} of the section's outline", i + 1);
    let made = compute(&outline.pieces, -inset, true, None, &name).map_err(SetInError::Vanishes)?;
    let m = made.made.len();
    if m == 0 {
        return Err(SetInError::Vanishes(
            "nothing of the outline is left".to_owned(),
        ));
    }
    if !made.left.is_empty() {
        return Err(SetInError::Splits(
            "a part of the outline narrower than twice the inset vanishes".to_owned(),
        ));
    }
    // Each result's curve with its direction along the loop.
    // (An arc runs counter-clockwise from its start; the rest from `from`.)
    let piece = |curve: &Made, from: P2, to: P2| -> Piece {
        let forward = match curve {
            Made::Arc { start, .. } => dist(*start, from) <= dist(*start, to),
            _ => true,
        };
        (made_curve(curve), forward)
    };
    let mut pieces = Vec::with_capacity(m);
    let mut seen = vec![false; m];
    let mut at = 0;
    // Round the loop from the first curve until it comes back.
    while !seen[at] {
        seen[at] = true;
        let (from, to) = made.ends[at];
        pieces.push(piece(&made.made[at], from, to));
        let next = made.next[at];
        // A round corner after it, from its end to the next one's start.
        if let (Some(after), Some((_, corner))) =
            (next, made.corners.iter().find(|(j, _)| *j == at))
        {
            pieces.push(piece(corner, to, made.ends[after].0));
        }
        match next {
            Some(after) => at = after,
            None if m == 1 => break,
            None => {
                return Err(SetInError::Vanishes(
                    "the outline set in does not close".to_owned(),
                ));
            }
        }
    }
    if m > 1 && at != 0 {
        return Err(SetInError::Vanishes(
            "the outline set in does not close".to_owned(),
        ));
    }
    // Curves the walk did not reach make other loops.
    if seen.iter().any(|s| !s) {
        let mut parts = 0;
        let mut visited = vec![false; m];
        for first in 0..m {
            let mut j = first;
            if visited[j] {
                continue;
            }
            parts += 1;
            while !visited[j] {
                visited[j] = true;
                match made.next[j] {
                    Some(after) => j = after,
                    None => break,
                }
            }
        }
        return Err(SetInError::Splits(format!(
            "the outline set in falls into {parts} parts"
        )));
    }
    Ok(Outline { pieces })
}

/// A corner of a loop that a chamfer cuts: a sharp corner where two
/// curves of any kind meet with a tangent break (`at` the shared point),
/// or the arc between two straight sides that rounds their corner (`at`
/// the sides' crossing), and how far its round reaches along each side
/// from the crossing (0 sharp). Only corners that turn outward (left on a
/// counter-clockwise loop) are cut: a chamfer at a corner that turns
/// inward would add to the plate.
struct LoopCorner {
    /// The arc's index for a rounded corner; None for a sharp one after
    /// `before`.
    arc: Option<usize>,
    before: usize,
    after: usize,
    at: P2,
    reach: f64,
}

/// The outward corners of a counter-clockwise loop.
fn corners(pieces: &[Piece]) -> Vec<LoopCorner> {
    let n = pieces.len();
    let line = |i: usize| matches!(pieces[i].0, Curve2::Line { .. });
    let mut out = Vec::new();
    if n < 2 {
        return out;
    }
    // Turning angles under this are no corner (a tangent join).
    let smooth = 1e-4;
    for i in 0..n {
        let next = (i + 1) % n;
        let open = !pieces[i].0.is_closed() && !pieces[next].0.is_closed();
        // A spline's or an ellipse's offset is a spline within the sketch
        // offset's tolerance: its tangent where it joins another curve
        // smoothly may be off by a little more.
        let exact = |j: usize| matches!(pieces[j].0, Curve2::Line { .. } | Curve2::Arc { .. });
        let least = if exact(i) && exact(next) {
            smooth
        } else {
            1e-3
        };
        if open
            && let (Some(d1), Some(d2)) = (tangent(&pieces[i], true), tangent(&pieces[next], false))
            && cross2(d1, d2).atan2(dot2(d1, d2)) > least
            && let Some((_, at)) = ends(&pieces[i])
        {
            out.push(LoopCorner {
                arc: None,
                before: i,
                after: next,
                at,
                reach: 0.0,
            });
        }
        let prev = (i + n - 1) % n;
        if n >= 3 && matches!(pieces[i].0, Curve2::Arc { .. }) && line(prev) && line(next) {
            let (Some(d1), Some(d2)) =
                (tangent(&pieces[prev], true), tangent(&pieces[next], false))
            else {
                continue;
            };
            let (Some(a_in), Some(a_out)) = (tangent(&pieces[i], false), tangent(&pieces[i], true))
            else {
                continue;
            };
            let tangent_in = cross2(d1, a_in).abs() < 1e-3 && dot2(d1, a_in) > 0.0;
            let tangent_out = cross2(a_out, d2).abs() < 1e-3 && dot2(a_out, d2) > 0.0;
            let turn = cross2(d1, d2).atan2(dot2(d1, d2));
            if !(tangent_in && tangent_out && turn > smooth && turn < PI - smooth) {
                continue;
            }
            let (_, p) = ends(&pieces[prev]).expect("a line has ends");
            let (q, _) = ends(&pieces[next]).expect("a line has ends");
            // Where the sides cross: p + s d1 = q − t d2.
            let s = cross2(sub2(q, p), d2) / cross2(d1, d2);
            out.push(LoopCorner {
                arc: Some(i),
                before: prev,
                after: next,
                at: along(p, d1, s),
                reach: s,
            });
        }
    }
    out
}

/// A curve between parameters `t0 < t1` of its own (a closed curve, which
/// has no corners, whole).
fn restricted(curve: &Curve2, t0: f64, t1: f64) -> Curve2 {
    match curve {
        Curve2::Line { .. } => Curve2::Line {
            a: curve.point(t0),
            b: curve.point(t1),
        },
        Curve2::Arc { center, radius, .. } => Curve2::Arc {
            center: *center,
            radius: *radius,
            start: t0,
            end: t1,
        },
        Curve2::EllipticalArc {
            center,
            major,
            minor,
            rotation,
            ..
        } => Curve2::EllipticalArc {
            center: *center,
            major: *major,
            minor: *minor,
            rotation: *rotation,
            start: t0,
            end: t1,
        },
        Curve2::Nurbs(n) => Curve2::Nurbs(n.piece(t0, t1)),
        Curve2::Circle { .. } | Curve2::Ellipse { .. } => curve.clone(),
    }
}

/// Where a chamfer `c` long cuts a curve of the loop that meets a corner
/// at its end along the loop (`at_end`) or its start: the first point
/// along it from the corner that lies `c` from the corner in a straight
/// line, and its parameter. None when no point of it lies that far.
fn cut_on_curve(piece: &Piece, at_end: bool, c: f64) -> Option<(P2, f64)> {
    let curve = &piece.0;
    let (lo, hi) = curve.domain();
    // The corner is at the parameter's end when the loop runs along it
    // and meets the corner at its end, or runs against it and meets the
    // corner at its start.
    let from_hi = at_end == piece.1;
    let corner_t = if from_hi { hi } else { lo };
    let corner = curve.point(corner_t);
    let mut params = curve.sample_params(lo, hi);
    if from_hi {
        params.reverse();
    }
    let mut before = corner_t;
    for t in params.into_iter().skip(1) {
        if dist(curve.point(t), corner) >= c {
            // Halved down to the double's precision.
            let (mut near, mut far) = (before, t);
            for _ in 0..200 {
                let mid = 0.5 * (near + far);
                if mid == near || mid == far {
                    break;
                }
                if dist(curve.point(mid), corner) >= c {
                    far = mid;
                } else {
                    near = mid;
                }
            }
            return Some((curve.point(far), far));
        }
        before = t;
    }
    None
}

/// The loop with each outward corner cut at the chamfer `c`: its legs `c`
/// long on either side from the corner in a straight line (from the
/// sides' crossing where an arc rounds a corner between two straight
/// sides); a curve cut there keeps the rest of itself. Refused in words
/// on a loop with no such corner, a chamfer shorter than a round reaches,
/// and one that leaves nothing of a side; `size` names the plate.
fn chamfered(outline: &Outline, c: f64, size: (f64, f64)) -> Result<Outline, String> {
    let pieces = &outline.pieces;
    let found = corners(pieces);
    if found.is_empty() {
        return Err(if outline.is_circle() {
            "a round section has no corners to chamfer".to_owned()
        } else {
            "the section's outline has no corners to chamfer: its curves all run on into each \
             other without a corner"
                .to_owned()
        });
    }
    let most = found.iter().map(|k| k.reach).fold(0.0, f64::max);
    if c < most - 1e-9 {
        return Err(format!(
            "the chamfer ({} mm) is less than the corners' round ({} mm): give at least {} mm",
            mm(c),
            mm(most),
            mm(most)
        ));
    }
    // Whole messages, for their translations.
    let nothing_left = |straight: bool| {
        if straight {
            format!(
                "a chamfer of {} mm leaves no straight side of the {} x {} mm plate",
                mm(c),
                mm(size.0),
                mm(size.1)
            )
        } else {
            format!(
                "a chamfer of {} mm leaves nothing of a curved side of the {} x {} mm plate",
                mm(c),
                mm(size.0),
                mm(size.1)
            )
        }
    };
    let n = pieces.len();
    let line = |i: usize| matches!(pieces[i].0, Curve2::Line { .. });
    let direction = |i: usize| tangent(&pieces[i], false).unwrap_or([1.0, 0.0]);
    // Where each piece now starts and ends along the loop: the point and,
    // on a curve, its parameter.
    let mut starts: Vec<Option<(P2, f64)>> = vec![None; n];
    let mut stops: Vec<Option<(P2, f64)>> = vec![None; n];
    // A cut in place of a rounding arc, and one after a piece.
    let mut replaces: Vec<Option<Piece>> = vec![None; n];
    let mut follows: Vec<Option<Piece>> = vec![None; n];
    // The cut on piece `i` `c` from the corner at its end or start.
    let cut = |i: usize, at_end: bool| -> Result<(P2, f64), String> {
        if line(i) {
            let (a, b) = ends(&pieces[i]).expect("a line has ends");
            let d = direction(i);
            Ok(if at_end {
                (along(b, d, -c), 0.0)
            } else {
                (along(a, d, c), 0.0)
            })
        } else {
            cut_on_curve(&pieces[i], at_end, c).ok_or_else(|| nothing_left(false))
        }
    };
    for k in &found {
        let (from, to) = match k.arc {
            Some(_) => (
                (along(k.at, direction(k.before), -c), 0.0),
                (along(k.at, direction(k.after), c), 0.0),
            ),
            None => (cut(k.before, true)?, cut(k.after, false)?),
        };
        stops[k.before] = Some(from);
        starts[k.after] = Some(to);
        let piece = (Curve2::Line { a: from.0, b: to.0 }, true);
        match k.arc {
            Some(arc) => replaces[arc] = Some(piece),
            None => follows[k.before] = Some(piece),
        }
    }
    let mut out = Vec::with_capacity(n + found.len());
    for (i, piece) in pieces.iter().enumerate() {
        if let Some(cut) = &replaces[i] {
            out.push(cut.clone());
            continue;
        }
        if line(i) {
            let (a, b) = ends(piece).expect("a line has ends");
            let a = starts[i].map_or(a, |s| s.0);
            let b = stops[i].map_or(b, |s| s.0);
            if dot2(sub2(b, a), direction(i)) <= 1e-6 {
                return Err(nothing_left(true));
            }
            out.push((Curve2::Line { a, b }, true));
        } else if starts[i].is_none() && stops[i].is_none() {
            out.push(piece.clone());
        } else {
            let (lo, hi) = piece.0.domain();
            // Along the parameter: from where the loop starts it to where
            // it stops it, or the other way round.
            let (first, last) = if piece.1 {
                (starts[i], stops[i])
            } else {
                (stops[i], starts[i])
            };
            let t0 = first.map_or(lo, |s| s.1);
            let t1 = last.map_or(hi, |s| s.1);
            if t1 - t0 <= 1e-9 * (hi - lo) {
                return Err(nothing_left(false));
            }
            out.push((restricted(&piece.0, t0, t1), piece.1));
        }
        if let Some(cut) = &follows[i] {
            out.push(cut.clone());
        }
    }
    Ok(Outline { pieces: out })
}

/// A closed section's plate: its outer loop set in by `inset` and its
/// corners chamfered by `chamfer` (none or 0: as the loop has them);
/// refused in words when it leaves no plate, the loop set in falls into
/// parts, or the chamfer does not fit.
pub(super) fn plate(
    section: &Section,
    outer: &Outline,
    inset: f64,
    chamfer: Option<f64>,
) -> Result<Plate, String> {
    let nothing = || {
        format!(
            "an inset of {} mm leaves nothing of the {} x {} mm section",
            mm(inset),
            mm(section.width()),
            mm(section.height())
        )
    };
    if 2.0 * inset >= section.width().min(section.height()) - 1e-6 {
        return Err(nothing());
    }
    let set = set_in(outer, inset).map_err(|e| match e {
        SetInError::Vanishes(e) => format!("{}: {e}", nothing()),
        SetInError::Splits(e) => format!(
            "an inset of {} mm splits the outline of the {} x {} mm section: {e}",
            mm(inset),
            mm(section.width()),
            mm(section.height())
        ),
    })?;
    let (min, max) = set.bounds();
    let (w, h) = (max[0] - min[0], max[1] - min[1]);
    if !(w > 1e-6 && h > 1e-6) || set.area() <= 1e-9 * w.max(h).powi(2) {
        return Err(nothing());
    }
    let closed = match chamfer.filter(|c| *c > 1e-9) {
        None => set,
        Some(c) => chamfered(&set, c, (w, h))?,
    };
    let (min, max) = closed.bounds();
    Ok(Plate {
        min,
        max,
        corners: [Corner::Round(0.0); 4],
        closed: Some(closed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::end_cap::plate_of;

    fn identity() -> Transform {
        Transform::IDENTITY
    }

    fn line(a: P2, b: P2) -> Curve3 {
        Curve3::Line {
            start: [a[0], a[1], 0.0],
            end: [b[0], b[1], 0.0],
        }
    }

    /// A regular hexagon `across` its flats, centred on the origin, its
    /// flats along x, as lines counter-clockwise or clockwise round it.
    fn hexagon(across: f64, clockwise: bool) -> Vec<Curve3> {
        let r = across / 3f64.sqrt();
        let at = |k: usize| {
            let t = PI / 3.0 * k as f64;
            [r * t.cos(), r * t.sin()]
        };
        let mut lines: Vec<Curve3> = (0..6)
            .map(|k| {
                if clockwise {
                    line(at(k + 1), at(k))
                } else {
                    line(at(k), at(k + 1))
                }
            })
            .collect();
        if clockwise {
            lines.reverse();
        }
        lines
    }

    /// A regular hexagon's area by its apothem.
    fn hexagon_area(apothem: f64) -> f64 {
        2.0 * 3f64.sqrt() * apothem * apothem
    }

    /// A flat oval (two half circles of radius `r` joined by straight
    /// sides `2 l` long), its half circles' normals `up` or down.
    fn oval(r: f64, l: f64, up: bool) -> Vec<Curve3> {
        let normal = [0.0, 0.0, if up { 1.0 } else { -1.0 }];
        let half = |cx: f64, x_axis: [f64; 3]| Curve3::Conic {
            center: [cx, 0.0, 0.0],
            normal,
            x_axis,
            major: r,
            minor: r,
            start: 0.0,
            end: PI,
            closed: false,
        };
        // Up: the right half from (l, −r) round to (l, r) has its x axis
        // along −y; down, along +y.
        let s = if up { 1.0 } else { -1.0 };
        vec![
            line([-l, -r], [l, -r]),
            half(l, [0.0, -s, 0.0]),
            line([l, r], [-l, r]),
            half(-l, [0.0, s, 0.0]),
        ]
    }

    fn oval_area(r: f64, l: f64) -> f64 {
        4.0 * r * l + PI * r * r
    }

    fn closed_plate(curves: &[Curve3], inset: Option<f64>, chamfer: Option<f64>) -> Plate {
        let outer = outer_loop(curves, &identity()).expect("a closed section");
        plate_of(&Section::closed(outer), 5.0, inset, None, chamfer).unwrap()
    }

    #[test]
    fn a_hexagon_s_plate_is_its_outline_set_in() {
        for clockwise in [false, true] {
            let plate = closed_plate(&hexagon(40.0, clockwise), None, None);
            let a = hexagon_area(20.0);
            assert!((plate.area() - a).abs() / a < 1e-12, "{}", plate.area());
            assert!((plate.width() - 80.0 / 3f64.sqrt()).abs() < 1e-9);
            assert!((plate.height() - 40.0).abs() < 1e-9);
            assert!(!plate.is_disc());
            assert_eq!(plate.region().loops[0].segments.len(), 6);
            let inset = closed_plate(&hexagon(40.0, clockwise), Some(1.5), None);
            let a = hexagon_area(18.5);
            assert!((inset.area() - a).abs() / a < 1e-9, "{}", inset.area());
            assert!((inset.height() - 37.0).abs() < 1e-9);
        }
    }

    #[test]
    fn a_hexagonal_tube_s_outer_loop_is_its_outline() {
        let mut curves = hexagon(40.0, false);
        curves.extend(hexagon(34.0, true));
        let plate = closed_plate(&curves, Some(1.0), None);
        let a = hexagon_area(19.0);
        assert!((plate.area() - a).abs() / a < 1e-9, "{}", plate.area());
    }

    #[test]
    fn a_hexagon_s_corners_take_a_chamfer() {
        // Each of the six corners (120 degrees inside) loses a triangle
        // with legs c: ½ c² sin 120°.
        let c = 3.0;
        let plate = closed_plate(&hexagon(40.0, false), None, Some(c));
        let a = hexagon_area(20.0) - 6.0 * 0.5 * c * c * (2.0 * PI / 3.0).sin();
        assert!((plate.area() - a).abs() / a < 1e-12, "{}", plate.area());
        assert_eq!(plate.region().loops[0].segments.len(), 12);
        // Legs as long as the side leave nothing straight.
        let side = 40.0 / 3f64.sqrt();
        let outer = outer_loop(&hexagon(40.0, false), &identity()).unwrap();
        let wide =
            plate_of(&Section::closed(outer), 5.0, None, None, Some(side / 2.0)).unwrap_err();
        assert!(wide.contains("leaves no straight side"), "{wide}");
    }

    #[test]
    fn an_oval_s_plate_is_its_outline_set_in_and_takes_no_chamfer() {
        for up in [true, false] {
            let plate = closed_plate(&oval(15.0, 15.0, up), None, None);
            let a = oval_area(15.0, 15.0);
            assert!((plate.area() - a).abs() / a < 1e-12, "{}", plate.area());
            assert!((plate.width() - 60.0).abs() < 1e-9);
            assert!((plate.height() - 30.0).abs() < 1e-9);
            let inset = closed_plate(&oval(15.0, 15.0, up), Some(2.0), None);
            let a = oval_area(13.0, 15.0);
            assert!((inset.area() - a).abs() / a < 1e-9, "{}", inset.area());
            let outer = outer_loop(&oval(15.0, 15.0, up), &identity()).unwrap();
            let refused =
                plate_of(&Section::closed(outer), 5.0, None, None, Some(2.0)).unwrap_err();
            assert!(refused.contains("no corners to chamfer"), "{refused}");
        }
    }

    #[test]
    fn an_ellipse_s_plate_is_its_outline_set_in() {
        let ellipse = Curve3::Conic {
            center: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 30.0,
            minor: 20.0,
            start: 0.0,
            end: TAU,
            closed: true,
        };
        let plate = closed_plate(std::slice::from_ref(&ellipse), None, None);
        let a = PI * 600.0;
        assert!((plate.area() - a).abs() / a < 1e-12, "{}", plate.area());
        // Set in by d: the area less d times the perimeter plus π d² (the
        // offset of a convex curve, d under its least radius of
        // curvature, 20² / 30), the offset a spline within the sketch
        // offset's tolerance.
        let d = 1.0;
        let inset = closed_plate(std::slice::from_ref(&ellipse), Some(d), None);
        let perimeter = {
            let steps = 100_000;
            (0..steps)
                .map(|i| {
                    let t = TAU * f64::from(i) / f64::from(steps);
                    let u = TAU * f64::from(i + 1) / f64::from(steps);
                    dist(
                        [30.0 * t.cos(), 20.0 * t.sin()],
                        [30.0 * u.cos(), 20.0 * u.sin()],
                    )
                })
                .sum::<f64>()
        };
        let expected = a - d * perimeter + PI * d * d;
        assert!(
            (inset.area() - expected).abs() / expected < 1e-4,
            "{} {expected}",
            inset.area()
        );
        let outer = outer_loop(std::slice::from_ref(&ellipse), &identity()).unwrap();
        let refused = plate_of(&Section::closed(outer), 5.0, None, None, Some(2.0)).unwrap_err();
        assert!(refused.contains("no corners to chamfer"), "{refused}");
    }

    #[test]
    fn a_round_outline_in_two_halves_makes_a_disc() {
        let half = |x_axis: [f64; 3]| Curve3::Conic {
            center: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            x_axis,
            major: 20.0,
            minor: 20.0,
            start: 0.0,
            end: PI,
            closed: false,
        };
        let plate = closed_plate(
            &[half([1.0, 0.0, 0.0]), half([-1.0, 0.0, 0.0])],
            Some(1.0),
            None,
        );
        assert!(plate.is_disc());
        assert!((plate.width() - 38.0).abs() < 1e-9);
        assert!((plate.area() - PI * 361.0).abs() < 1e-9);
        let outer = outer_loop(
            &[half([1.0, 0.0, 0.0]), half([-1.0, 0.0, 0.0])],
            &identity(),
        )
        .unwrap();
        let refused = plate_of(&Section::closed(outer), 5.0, None, None, Some(2.0)).unwrap_err();
        assert!(
            refused.contains("round section has no corners"),
            "{refused}"
        );
    }

    #[test]
    fn a_square_tube_s_outline_keeps_its_rounds_and_their_chamfer() {
        // 40 x 40, corners rounded by 6, as lines and quarter arcs.
        let (h, r) = (20.0, 6.0);
        let arc = |cx: f64, cy: f64, from: f64| Curve3::Conic {
            center: [cx, cy, 0.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: r,
            minor: r,
            start: from,
            end: from + PI / 2.0,
            closed: false,
        };
        let o = h - r;
        let curves = &[
            line([h, -o], [h, o]),
            arc(o, o, 0.0),
            line([o, h], [-o, h]),
            arc(-o, o, PI / 2.0),
            line([-h, o], [-h, -o]),
            arc(-o, -o, PI),
            line([-o, -h], [o, -h]),
            arc(o, -o, 1.5 * PI),
        ];
        let plate = closed_plate(curves, Some(1.0), None);
        let a = 38.0 * 38.0 - (4.0 - PI) * 25.0;
        assert!((plate.area() - a).abs() / a < 1e-9, "{}", plate.area());
        // The chamfer at least the round, as the rectangle's plate.
        let chamfered = closed_plate(curves, None, Some(6.0));
        assert!((chamfered.area() - (1600.0 - 72.0)).abs() < 1e-9);
        let outer = outer_loop(curves, &identity()).unwrap();
        let small = plate_of(&Section::closed(outer), 5.0, None, None, Some(3.0)).unwrap_err();
        assert!(
            small.contains("less than the corners' round (6 mm)"),
            "{small}"
        );
    }

    #[test]
    fn an_angle_s_outline_is_open() {
        // An L 50 x 50 x 5 with sharp corners: not convex.
        let points = [
            [0.0, 0.0],
            [50.0, 0.0],
            [50.0, 5.0],
            [5.0, 5.0],
            [5.0, 50.0],
            [0.0, 50.0],
        ];
        let curves: Vec<Curve3> = (0..6)
            .map(|k| line(points[k], points[(k + 1) % 6]))
            .collect();
        assert!(outer_loop(&curves, &identity()).is_none());
    }

    /// A closed polygon's sides, in the points' order.
    fn polygon_lines(points: &[P2]) -> Vec<Curve3> {
        let n = points.len();
        (0..n)
            .map(|k| line(points[k], points[(k + 1) % n]))
            .collect()
    }

    /// An L 60 x 60 with legs 20 wide, its heel at the origin, as a tube
    /// with a 3 mm wall (its inside listed clockwise), or solid.
    fn l_section(hollow: bool) -> Vec<Curve3> {
        let mut curves = polygon_lines(&[
            [0.0, 0.0],
            [60.0, 0.0],
            [60.0, 20.0],
            [20.0, 20.0],
            [20.0, 60.0],
            [0.0, 60.0],
        ]);
        if hollow {
            curves.extend(polygon_lines(&[
                [3.0, 3.0],
                [3.0, 57.0],
                [17.0, 57.0],
                [17.0, 17.0],
                [57.0, 17.0],
                [57.0, 3.0],
            ]));
        }
        curves
    }

    /// The L's outline set in by `r`: its area less `r` times its length
    /// (240), plus r² for each of its five right-angled outward corners
    /// (trimmed), less the quarter circle of radius r about its inward
    /// corner (rounded).
    fn l_area(r: f64) -> f64 {
        2000.0 - 240.0 * r + 5.0 * r * r - 0.25 * PI * r * r
    }

    #[test]
    fn an_l_shaped_tube_is_closed_and_its_outline_set_in() {
        let curves = l_section(true);
        let outer = outer_loop(&curves, &identity()).expect("a hollow section is closed");
        assert!((outer.area() - 2000.0).abs() < 1e-9);
        for r in [0.0, 1.0, 2.5] {
            let plate = closed_plate(&curves, Some(r), None);
            let a = l_area(r);
            assert!((plate.area() - a).abs() / a < 1e-9, "{r}: {}", plate.area());
        }
        // The plate's box: the L's set in.
        let plate = closed_plate(&curves, Some(1.0), None);
        assert!((plate.width() - 58.0).abs() < 1e-9);
        assert!((plate.height() - 58.0).abs() < 1e-9);
        // A chamfer of 3 cuts the five outward corners, not the inward one.
        let chamfered = closed_plate(&curves, None, Some(3.0));
        let a = 2000.0 - 5.0 * 4.5;
        assert!(
            (chamfered.area() - a).abs() / a < 1e-12,
            "{}",
            chamfered.area()
        );
        assert_eq!(chamfered.region().loops[0].segments.len(), 11);
    }

    #[test]
    fn a_solid_l_or_one_with_a_bore_is_open() {
        // A solid L is an angle.
        assert!(outer_loop(&l_section(false), &identity()).is_none());
        // A bore in its corner is no wall's inside (a slot profile's core
        // bore).
        let mut curves = l_section(false);
        curves.push(Curve3::Conic {
            center: [10.0, 10.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 4.0,
            minor: 4.0,
            start: 0.0,
            end: TAU,
            closed: true,
        });
        assert!(outer_loop(&curves, &identity()).is_none());
        // Two holes (two chambers) make no single wall either.
        let mut curves = l_section(false);
        curves.extend(polygon_lines(&[
            [3.0, 3.0],
            [3.0, 17.0],
            [17.0, 17.0],
            [17.0, 3.0],
        ]));
        curves.extend(polygon_lines(&[
            [23.0, 3.0],
            [23.0, 17.0],
            [57.0, 17.0],
            [57.0, 3.0],
        ]));
        assert!(outer_loop(&curves, &identity()).is_none());
    }

    #[test]
    fn an_inset_that_splits_the_outline_is_refused_in_words() {
        // Two 30 x 30 squares joined by a neck 4 wide: an inset of 3
        // leaves two squares apart.
        let points = [
            [0.0, 0.0],
            [30.0, 0.0],
            [30.0, 13.0],
            [40.0, 13.0],
            [40.0, 0.0],
            [70.0, 0.0],
            [70.0, 30.0],
            [40.0, 30.0],
            [40.0, 17.0],
            [30.0, 17.0],
            [30.0, 30.0],
            [0.0, 30.0],
        ];
        let outer = Outline {
            pieces: polygon_lines(&points)
                .iter()
                .filter_map(|c| planar(c, &identity()))
                .collect(),
        };
        let section = Section::closed(outer);
        let fits = plate_of(&section, 5.0, Some(1.0), None, None).unwrap();
        assert!(fits.area() > 0.0);
        let refused = plate_of(&section, 5.0, Some(3.0), None, None).unwrap_err();
        assert!(refused.contains("splits the outline"), "{refused}");
    }

    /// A half disc of radius `r` on the x axis (a D), its diameter along
    /// x: a line and a half circle, with a corner where they meet.
    fn half_disc(r: f64) -> Vec<Curve3> {
        vec![
            line([-r, 0.0], [r, 0.0]),
            Curve3::Conic {
                center: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
                x_axis: [1.0, 0.0, 0.0],
                major: r,
                minor: r,
                start: 0.0,
                end: PI,
                closed: false,
            },
        ]
    }

    /// What a chamfer of legs `c` takes off a corner between a straight
    /// side and an arc of radius `r` meeting at right angles: the triangle
    /// of its legs and the arc's segment cut off by the chord `c` long.
    fn line_arc_corner(r: f64, c: f64) -> f64 {
        let theta = 2.0 * (0.5 * c / r).asin();
        0.5 * c * r * theta.sin() + 0.5 * r * r * (theta - theta.sin())
    }

    #[test]
    fn corners_between_a_line_and_an_arc_take_a_chamfer() {
        let (r, c) = (20.0, 3.0);
        let plate = closed_plate(&half_disc(r), None, Some(c));
        let a = 0.5 * PI * r * r - 2.0 * line_arc_corner(r, c);
        assert!((plate.area() - a).abs() / a < 1e-12, "{} {a}", plate.area());
        // The line, its two cuts and what is left of the arc.
        assert_eq!(plate.region().loops[0].segments.len(), 4);
        // Legs longer than half the diameter leave nothing straight.
        let outer = outer_loop(&half_disc(r), &identity()).unwrap();
        let wide = plate_of(&Section::closed(outer), 5.0, None, None, Some(21.0)).unwrap_err();
        assert!(wide.contains("leaves no straight side"), "{wide}");
    }

    #[test]
    fn corners_between_two_arcs_take_a_chamfer() {
        // A lens: the arcs of radius 20 about (0, ∓10) meeting at
        // (±√300, 0), each 120 degrees.
        let r = 20.0;
        let arc = |cy: f64, from: f64| Curve3::Conic {
            center: [0.0, cy, 0.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: r,
            minor: r,
            start: from,
            end: from + 2.0 * PI / 3.0,
            closed: false,
        };
        let curves = [arc(-10.0, PI / 6.0), arc(10.0, 7.0 * PI / 6.0)];
        let lens = r * r * (2.0 * PI / 3.0 - 0.75f64.sqrt());
        let plain = closed_plate(&curves, None, None);
        assert!((plain.area() - lens).abs() / lens < 1e-12);
        // Each tip loses the triangle between it and the two cuts, a chord
        // c along each arc, and the two arcs' segments off those chords.
        let c = 4.0;
        let theta = 2.0 * (0.5 * c / r).asin();
        let tip = [300f64.sqrt(), 0.0];
        let cut = [
            r * (PI / 6.0 + theta).cos(),
            -10.0 + r * (PI / 6.0 + theta).sin(),
        ];
        let triangle = cut[1] * (tip[0] - cut[0]);
        let segment = 0.5 * r * r * (theta - theta.sin());
        let a = lens - 2.0 * (triangle + 2.0 * segment);
        let plate = closed_plate(&curves, None, Some(c));
        assert!((plate.area() - a).abs() / a < 1e-12, "{} {a}", plate.area());
        assert_eq!(plate.region().loops[0].segments.len(), 4);
    }
}
