// SPDX-License-Identifier: MIT
//! Cut sheets (mitcad#853): the outlines of flat parts at 1:1, placed on a
//! sheet of material and written as DXF and SVG for cutting.
//!
//! A part's outline is its body's silhouette along the normal of one of
//! its planar faces: the kernel projects the body along that normal with
//! hidden lines removed (`Kernel::project_view`, OCCT's hidden-line
//! removal), and [`silhouette`] keeps of the lines it sees those that
//! bound the body's shadow. The lines seen split the view's plane into
//! regions; each region is asked once whether the body lies behind it (a
//! line of sight through it crosses the body, `Kernel::segments_crossings`
//! in the api), and a line with material on one side only is a cut. So a
//! through hole is one inner loop however many faces its wall has, a blind
//! hole or a pocket is none, and the outer boundary is the part's whole
//! extent along the normal. Lines and arcs stay lines and arcs, and
//! B-splines (the projection's B-spline and Bézier edges, other curves
//! within 1e-6 mm) stay B-splines: split and joined on the curve itself,
//! cut by knot insertion ([`super::bspline`]), written as DXF `SPLINE`s of
//! their own degree, knots, poles and weights and as cubic Béziers in SVG
//! paths. Polylines (from the polygonal projection) are written as splines
//! of degree 1 in DXF and as lines in SVG.
//!
//! The loops are in the part's view coordinates (millimetres); a part is
//! placed on the sheet by the middle of its outline's box and turned about
//! it ([`placed`]). The writers put outer boundaries on the layer
//! `CUT_OUTER` and inner loops on `CUT_INNER` (the Flat Pattern's names),
//! and the parts' names on `LABEL` when asked.
//!
//! A part can be a sheet-metal body's Flat Pattern instead (mitcad#1980,
//! [`flat_part_view`]): its developed outline as cut edges, and its bend
//! lines on the Flat Pattern's layers `BEND_UP` and `BEND_DOWN`.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};
use std::fmt::Write as _;
use std::sync::Arc;

use mitcad_model::sheet::flat_dxf::{
    BEND_DOWN, BEND_UP, CUT_INNER, CUT_OUTER, FlatLoop, FlatPiece,
};

use super::bspline::{self, Spline2};
use super::sheet::{Anchor, Group, Item, Pen, Sheet, text_width};
use super::{P2, ViewCurve, ViewEdge, trim_number};
use mitcad_model::drawing::cut_sheet_def::MarkKind;
use mitcad_model::sheet::process::{OutputPreset, SheetProcess};

/// The layer of the parts' names.
pub const LABEL: &str = "LABEL";

// --- Pieces of outlines -----------------------------------------------------

/// A piece of an outline in a plane, mm.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Line(P2, P2),
    /// From `start` (radians) by `sweep`: counter-clockwise when positive,
    /// clockwise when negative; a whole circle sweeps 2 pi.
    Arc {
        center: P2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
    /// At least two points.
    Polyline(Vec<P2>),
    /// A B-spline the projection gave, from its parameter `from` to `to`
    /// (backwards when `to < from`): cut, turned and written as the curve
    /// it is (mitcad#853).
    Spline {
        curve: Arc<SplineCurve>,
        from: f64,
        to: f64,
    },
}

/// How far a B-spline's points for drawing and crossings may lie from it,
/// mm; crossings and ends on it are then found on the curve itself.
const SPLINE_SAMPLES: f64 = 1e-3;
/// How far an SVG path's lines may lie from a B-spline that is rational or
/// of a degree over 3 (one of degree 3 or less is written as cubic
/// Béziers, exactly), mm.
pub const SVG_DEFLECTION: f64 = 1e-4;

/// A B-spline with points of it along its whole range and their
/// parameters (within [`SPLINE_SAMPLES`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SplineCurve {
    pub spline: Spline2,
    samples: Vec<(f64, P2)>,
}

impl SplineCurve {
    pub fn new(spline: Spline2) -> Self {
        let (a, b) = spline.domain();
        let samples = spline.samples(a, b, SPLINE_SAMPLES);
        Self { spline, samples }
    }

    /// Its points from `from` to `to` with their parameters, the ends
    /// exactly.
    fn points(&self, from: f64, to: f64) -> Vec<(f64, P2)> {
        let (lo, hi) = (from.min(to), from.max(to));
        let mut out = vec![(lo, self.spline.point(lo))];
        out.extend(
            self.samples
                .iter()
                .filter(|(u, _)| *u > lo && *u < hi)
                .copied(),
        );
        out.push((hi, self.spline.point(hi)));
        if from > to {
            out.reverse();
        }
        out
    }
}

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(a: P2, s: f64) -> P2 {
    [a[0] * s, a[1] * s]
}

fn cross2(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot2(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn norm(a: P2) -> f64 {
    a[0].hypot(a[1])
}

fn dist(a: P2, b: P2) -> f64 {
    norm(sub(a, b))
}

fn lerp(a: P2, b: P2, t: f64) -> P2 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn polar(center: P2, radius: f64, angle: f64) -> P2 {
    [
        center[0] + radius * angle.cos(),
        center[1] + radius * angle.sin(),
    ]
}

impl Piece {
    /// The end of its parameter's range: 1 for a line or an arc, the last
    /// point's index for a polyline.
    pub fn domain(&self) -> f64 {
        match self {
            Self::Polyline(points) => (points.len().max(2) - 1) as f64,
            _ => 1.0,
        }
    }

    /// The point at a parameter in `[0, domain]`.
    pub fn point(&self, t: f64) -> P2 {
        match self {
            Self::Line(a, b) => lerp(*a, *b, t),
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => polar(*center, *radius, start + sweep * t),
            Self::Polyline(points) => {
                let last = points.len() - 1;
                let t = t.clamp(0.0, last as f64);
                let i = (t.floor() as usize).min(last.saturating_sub(1));
                lerp(points[i], points[(i + 1).min(last)], t - i as f64)
            }
            Self::Spline { curve, from, to } => curve.spline.point(from + (to - from) * t),
        }
    }

    pub fn start(&self) -> P2 {
        self.point(0.0)
    }

    pub fn end(&self) -> P2 {
        self.point(self.domain())
    }

    pub fn length(&self) -> f64 {
        match self {
            Self::Line(a, b) => dist(*a, *b),
            Self::Arc { radius, sweep, .. } => radius * sweep.abs(),
            Self::Polyline(points) => points.windows(2).map(|w| dist(w[0], w[1])).sum(),
            Self::Spline { curve, from, to } => curve
                .points(*from, *to)
                .windows(2)
                .map(|w| dist(w[0].1, w[1].1))
                .sum(),
        }
    }

    pub fn reversed(&self) -> Self {
        match self {
            Self::Line(a, b) => Self::Line(*b, *a),
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => Self::Arc {
                center: *center,
                radius: *radius,
                start: start + sweep,
                sweep: -sweep,
            },
            Self::Polyline(points) => Self::Polyline(points.iter().rev().copied().collect()),
            Self::Spline { curve, from, to } => Self::Spline {
                curve: Arc::clone(curve),
                from: *to,
                to: *from,
            },
        }
    }

    /// The part between two parameters (`t0 < t1`).
    fn slice(&self, t0: f64, t1: f64) -> Self {
        match self {
            Self::Line(..) => Self::Line(self.point(t0), self.point(t1)),
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => Self::Arc {
                center: *center,
                radius: *radius,
                start: start + sweep * t0,
                sweep: sweep * (t1 - t0),
            },
            Self::Polyline(points) => {
                let mut out = vec![self.point(t0)];
                let first = t0.floor() as usize + 1;
                for (i, p) in points.iter().enumerate().skip(first) {
                    if (i as f64) >= t1 {
                        break;
                    }
                    out.push(*p);
                }
                out.push(self.point(t1));
                Self::Polyline(out)
            }
            Self::Spline { curve, from, to } => Self::Spline {
                curve: Arc::clone(curve),
                from: from + (to - from) * t0,
                to: from + (to - from) * t1,
            },
        }
    }

    /// The point this far along it from its start (its end beyond it).
    fn at_distance(&self, d: f64) -> P2 {
        match self {
            Self::Line(a, b) => {
                let l = dist(*a, *b);
                if l <= 0.0 {
                    *a
                } else {
                    lerp(*a, *b, (d / l).min(1.0))
                }
            }
            Self::Arc { radius, sweep, .. } => {
                let whole = radius * sweep.abs();
                self.point(if whole > 0.0 {
                    (d / whole).min(1.0)
                } else {
                    0.0
                })
            }
            Self::Polyline(points) => {
                let mut left = d;
                for w in points.windows(2) {
                    let l = dist(w[0], w[1]);
                    if l >= left && l > 0.0 {
                        return lerp(w[0], w[1], left / l);
                    }
                    left -= l;
                }
                *points.last().expect("a polyline's points")
            }
            Self::Spline { curve, from, to } => {
                let points: Vec<P2> = curve.points(*from, *to).iter().map(|(_, p)| *p).collect();
                Self::Polyline(points).at_distance(d)
            }
        }
    }

    /// Its middle and the unit direction it runs in there.
    fn middle(&self) -> (P2, P2) {
        let unit = |v: P2| {
            let l = norm(v);
            if l > 0.0 {
                scale(v, 1.0 / l)
            } else {
                [1.0, 0.0]
            }
        };
        match self {
            Self::Line(a, b) => (lerp(*a, *b, 0.5), unit(sub(*b, *a))),
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let angle = start + sweep / 2.0;
                let s = sweep.signum();
                (
                    polar(*center, *radius, angle),
                    [-angle.sin() * s, angle.cos() * s],
                )
            }
            Self::Polyline(points) => {
                // The middle of its longest segment.
                let w = points
                    .windows(2)
                    .max_by(|a, b| dist(a[0], a[1]).total_cmp(&dist(b[0], b[1])))
                    .expect("a polyline's segment");
                (lerp(w[0], w[1], 0.5), unit(sub(w[1], w[0])))
            }
            Self::Spline { .. } => {
                let ahead = sub(self.point(0.5 + 1e-4), self.point(0.5 - 1e-4));
                let way = if norm(ahead) > 0.0 {
                    ahead
                } else {
                    sub(self.end(), self.start())
                };
                (self.point(0.5), unit(way))
            }
        }
    }

    /// Its points after its start, arcs in steps of at most 2 degrees.
    fn flatten_into(&self, out: &mut Vec<P2>) {
        match self {
            Self::Line(_, b) => out.push(*b),
            Self::Arc { sweep, .. } => {
                let steps = ((sweep.abs() / (2f64.to_radians())).ceil() as usize).max(1);
                for i in 1..=steps {
                    out.push(self.point(i as f64 / steps as f64));
                }
            }
            Self::Polyline(points) => out.extend(points.iter().skip(1)),
            Self::Spline { curve, from, to } => {
                out.extend(curve.points(*from, *to).iter().skip(1).map(|(_, p)| *p));
            }
        }
    }

    /// Turned by `angle` about the origin and then moved by `by`.
    fn moved(&self, angle: f64, by: P2) -> Self {
        let (s, c) = angle.sin_cos();
        let f = |p: P2| [c * p[0] - s * p[1] + by[0], s * p[0] + c * p[1] + by[1]];
        match self {
            Self::Line(a, b) => Self::Line(f(*a), f(*b)),
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => Self::Arc {
                center: f(*center),
                radius: *radius,
                start: start + angle,
                sweep: *sweep,
            },
            Self::Polyline(points) => Self::Polyline(points.iter().map(|p| f(*p)).collect()),
            Self::Spline { curve, from, to } => Self::Spline {
                curve: Arc::new(SplineCurve {
                    spline: curve.spline.mapped(f),
                    samples: curve.samples.iter().map(|(u, p)| (*u, f(*p))).collect(),
                }),
                from: *from,
                to: *to,
            },
        }
    }

    /// A B-spline piece as the curve it is, from its start to its end;
    /// None for another piece.
    pub fn exact(&self) -> Option<Spline2> {
        match self {
            Self::Spline { curve, from, to } => Some(if from <= to {
                curve.spline.segment(*from, *to)
            } else {
                curve.spline.segment(*to, *from).reversed()
            }),
            _ => None,
        }
    }

    /// The way it runs at a parameter (not a unit vector).
    fn derivative(&self, t: f64) -> P2 {
        const H: f64 = 1e-6;
        let (a, b) = ((t - H).max(0.0), (t + H).min(self.domain()));
        if b <= a {
            return [0.0, 0.0];
        }
        scale(sub(self.point(b), self.point(a)), 1.0 / (b - a))
    }

    /// On a B-spline, the parameter of the point nearest `p` from a guess
    /// (Newton's method on the curve itself); others' as given.
    fn refined_nearest(&self, p: P2, guess: f64) -> f64 {
        if !matches!(self, Self::Spline { .. }) {
            return guess;
        }
        let mut t = guess;
        for _ in 0..30 {
            let d = self.derivative(t);
            let dd = dot2(d, d);
            if dd <= 0.0 {
                break;
            }
            let step = dot2(sub(p, self.point(t)), d) / dd;
            t = (t + step).clamp(0.0, self.domain());
            if step.abs() < 1e-15 {
                break;
            }
        }
        t
    }
}

/// Where two pieces cross, from a guess found on their lines: on the
/// curves themselves when either is a B-spline (Newton's method), else as
/// found.
fn refined_crossing(a: &Piece, b: &Piece, ta: f64, tb: f64) -> (f64, f64) {
    let spline = |p: &Piece| matches!(p, Piece::Spline { .. });
    if !spline(a) && !spline(b) {
        return (ta, tb);
    }
    let (mut s, mut t) = (ta, tb);
    for _ in 0..30 {
        let f = sub(a.point(s), b.point(t));
        if norm(f) < 1e-13 {
            break;
        }
        let (da, db) = (a.derivative(s), b.derivative(t));
        // [da, -db] [ds, dt] = -f
        let det = -da[0] * db[1] + db[0] * da[1];
        if det.abs() < 1e-300 {
            break;
        }
        let ds = (-f[0] * -db[1] + db[0] * -f[1]) / det;
        let dt = (da[0] * -f[1] + f[0] * da[1]) / det;
        s = (s + ds).clamp(0.0, a.domain());
        t = (t + dt).clamp(0.0, b.domain());
    }
    if dist(a.point(s), b.point(t)) < 1e-9 {
        (s, t)
    } else {
        (ta, tb)
    }
}

/// A closed loop of an outline: its pieces end to end, the material on
/// their left (an outer boundary runs counter-clockwise, an inner loop
/// clockwise).
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineLoop {
    pub outer: bool,
    pub pieces: Vec<Piece>,
}

impl OutlineLoop {
    /// Its points, arcs flattened, the first not repeated at the end.
    pub fn polygon(&self) -> Vec<P2> {
        let mut out = Vec::new();
        if let Some(first) = self.pieces.first() {
            out.push(first.start());
        }
        for piece in &self.pieces {
            piece.flatten_into(&mut out);
        }
        if out.len() > 1 && dist(out[0], *out.last().expect("a point")) < 1e-9 {
            out.pop();
        }
        out
    }

    /// The area it encloses, positive when it runs counter-clockwise.
    pub fn signed_area(&self) -> f64 {
        signed_area(&self.polygon())
    }

    pub fn length(&self) -> f64 {
        self.pieces.iter().map(Piece::length).sum()
    }
}

fn signed_area(polygon: &[P2]) -> f64 {
    let n = polygon.len();
    (0..n)
        .map(|i| cross2(polygon[i], polygon[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

/// A part's outline: its outer boundaries first, then its inner loops,
/// each kind the largest first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outline {
    pub loops: Vec<OutlineLoop>,
}

impl Outline {
    pub fn outer(&self) -> usize {
        self.loops.iter().filter(|l| l.outer).count()
    }

    pub fn inner(&self) -> usize {
        self.loops.len() - self.outer()
    }

    /// Its box: lower left and upper right corners.
    pub fn bounds(&self) -> Option<(P2, P2)> {
        bounds(&self.loops)
    }
}

/// A part worked out from its body: the face it is seen along and its
/// outline there, with the body's planar faces to choose from; or its
/// Flat Pattern's outline and bend lines (mitcad#1980).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartView {
    /// The face's topological name; empty for a Flat Pattern.
    pub face: String,
    /// The body's planar faces with their areas (mm²), in the kernel's
    /// order.
    pub faces: Vec<(String, f64)>,
    pub outline: Outline,
    /// A Flat Pattern's bend lines, in the outline's coordinates.
    pub bends: Vec<BendLine>,
    /// Where the outline's coordinates lie in the body's component
    /// (mitcad#1939), for the marks sketches lay on the part; None when not
    /// known.
    pub frame: Option<MarkFrame>,
    /// Marks of the part itself in the outline's coordinates: a kerf
    /// bend's slits on a Flat Pattern part (mitcad#1939).
    pub own_marks: Vec<Mark>,
}

/// The outline's coordinates of a part in its component (mitcad#1939): a
/// point `q` of the component (moved by `transform` first, where the part's
/// occurrence places it) lies at `(dot(q - origin, x), dot(q - origin, y))`,
/// the first negated when `mirror`.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkFrame {
    pub transform: Option<mitcad_model::Transform>,
    pub origin: [f64; 3],
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub mirror: bool,
}

impl MarkFrame {
    pub fn point(&self, q: [f64; 3]) -> P2 {
        let q = match &self.transform {
            Some(t) => t.apply_point(q),
            None => q,
        };
        let d = [
            q[0] - self.origin[0],
            q[1] - self.origin[1],
            q[2] - self.origin[2],
        ];
        let dot = |a: [f64; 3]| a[0] * d[0] + a[1] * d[1] + a[2] * d[2];
        let x = dot(self.x);
        [if self.mirror { -x } else { x }, dot(self.y)]
    }

    /// Polylines of the component laid on the part as marks of `kind`.
    pub fn marks(&self, kind: MarkKind, polylines: &[Vec<[f64; 3]>]) -> Vec<Mark> {
        polylines
            .iter()
            .filter(|l| l.len() > 1)
            .map(|l| Mark {
                kind,
                piece: if l.len() == 2 {
                    Piece::Line(self.point(l[0]), self.point(l[1]))
                } else {
                    Piece::Polyline(l.iter().map(|q| self.point(*q)).collect())
                },
            })
            .collect()
    }
}

/// A bend line of a Flat Pattern part (mitcad#1980): written on
/// `BEND_UP` when the bend's inside is on the side the pattern is seen
/// from, else on `BEND_DOWN`, as the Flat Pattern's DXF has them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BendLine {
    pub start: P2,
    pub end: P2,
    pub up: bool,
}

/// A Flat Pattern as a cut sheet's part (mitcad#1980): its loops put end
/// to end, each exterior running counter-clockwise and each hole
/// clockwise, the exteriors first and each kind the largest first, and
/// its bend lines.
pub fn flat_part_view(loops: &[FlatLoop], bends: Vec<BendLine>) -> PartView {
    let mut out: Vec<OutlineLoop> = loops
        .iter()
        .filter(|l| !l.pieces.is_empty())
        .map(|l| {
            let pieces: Vec<Piece> = l
                .pieces
                .iter()
                .map(|piece| match piece {
                    FlatPiece::Line(a, b) => Piece::Line(*a, *b),
                    FlatPiece::Arc {
                        center,
                        radius,
                        start,
                        sweep,
                    } => Piece::Arc {
                        center: *center,
                        radius: *radius,
                        start: *start,
                        sweep: *sweep,
                    },
                    FlatPiece::Polyline(points) => Piece::Polyline(points.clone()),
                })
                .collect();
            let mut found = OutlineLoop {
                outer: l.outer,
                pieces: end_to_end(pieces),
            };
            // The material on the pieces' left.
            if (found.signed_area() > 0.0) != found.outer {
                found.pieces = found.pieces.iter().rev().map(Piece::reversed).collect();
            }
            found
        })
        .collect();
    out.sort_by(|a, b| {
        b.outer.cmp(&a.outer).then(
            b.signed_area()
                .abs()
                .partial_cmp(&a.signed_area().abs())
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    PartView {
        face: String::new(),
        faces: Vec::new(),
        outline: Outline { loops: out },
        bends,
        frame: None,
        own_marks: Vec::new(),
    }
}

/// A loop's pieces each turned to start where the one before it ends.
fn end_to_end(mut pieces: Vec<Piece>) -> Vec<Piece> {
    const JOIN: f64 = 1e-6;
    if pieces.len() < 2 {
        return pieces;
    }
    // The first runs towards the second.
    let (second_start, second_end) = (pieces[1].start(), pieces[1].end());
    let first_end = pieces[0].end();
    if dist(first_end, second_start) > JOIN && dist(first_end, second_end) > JOIN {
        pieces[0] = pieces[0].reversed();
    }
    for i in 1..pieces.len() {
        let at = pieces[i - 1].end();
        if dist(pieces[i].start(), at) > JOIN && dist(pieces[i].end(), at) <= JOIN {
            pieces[i] = pieces[i].reversed();
        }
    }
    pieces
}

/// The box of loops: lower left and upper right corners.
pub fn bounds(loops: &[OutlineLoop]) -> Option<(P2, P2)> {
    let mut out: Option<(P2, P2)> = None;
    for l in loops {
        for p in l.polygon() {
            out = Some(match out {
                None => (p, p),
                Some((lo, hi)) => (
                    [lo[0].min(p[0]), lo[1].min(p[1])],
                    [hi[0].max(p[0]), hi[1].max(p[1])],
                ),
            });
        }
    }
    out
}

// --- The silhouette ---------------------------------------------------------

/// A line or an arc of a piece, for crossings.
#[derive(Debug, Clone, Copy)]
enum Prim {
    Seg(P2, P2),
    Arc {
        center: P2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
}

impl Prim {
    fn length(&self) -> f64 {
        match self {
            Self::Seg(a, b) => dist(*a, *b),
            Self::Arc { radius, sweep, .. } => radius * sweep.abs(),
        }
    }

    /// The parameter in [0, 1] of an angle on the arc's circle, or None
    /// off the arc (beyond `slack` radians of it).
    fn arc_parameter(start: f64, sweep: f64, angle: f64, slack: f64) -> Option<f64> {
        let span = sweep.abs();
        if span <= 0.0 {
            return None;
        }
        let mut raw = ((angle - start) * sweep.signum()).rem_euclid(TAU);
        if raw > span + slack && raw > TAU - slack {
            raw -= TAU;
        }
        (raw >= -slack && raw <= span + slack).then_some(raw / span)
    }

    /// The parameter of the point nearest `p`, and its distance.
    fn nearest(&self, p: P2) -> (f64, f64) {
        match *self {
            Self::Seg(a, b) => {
                let d = sub(b, a);
                let l2 = dot2(d, d);
                let t = if l2 > 0.0 {
                    (dot2(sub(p, a), d) / l2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (t, dist(p, lerp(a, b, t)))
            }
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let angle = (p[1] - center[1]).atan2(p[0] - center[0]);
                match Self::arc_parameter(start, sweep, angle, 0.0) {
                    Some(t) => (t, (dist(p, center) - radius).abs()),
                    None => {
                        let a = polar(center, radius, start);
                        let b = polar(center, radius, start + sweep);
                        if dist(p, a) <= dist(p, b) {
                            (0.0, dist(p, a))
                        } else {
                            (1.0, dist(p, b))
                        }
                    }
                }
            }
        }
    }
}

/// Where two lines or arcs cross: the parameters on each.
fn crossings(a: &Prim, b: &Prim, tol: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let slack_of = |p: &Prim| tol / p.length().max(tol);
    match (*a, *b) {
        (Prim::Seg(p, q), Prim::Seg(r, s)) => {
            let d1 = sub(q, p);
            let d2 = sub(s, r);
            let den = cross2(d1, d2);
            if den.abs() <= 1e-12 * norm(d1) * norm(d2) {
                return out;
            }
            let w = sub(r, p);
            let u = cross2(w, d2) / den;
            let v = cross2(w, d1) / den;
            let (ea, eb) = (slack_of(a), slack_of(b));
            if u >= -ea && u <= 1.0 + ea && v >= -eb && v <= 1.0 + eb {
                out.push((u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)));
            }
        }
        (Prim::Seg(..), Prim::Arc { .. }) => {
            for (v, u) in crossings(b, a, tol) {
                out.push((u, v));
            }
        }
        (
            Prim::Arc {
                center,
                radius,
                start,
                sweep,
            },
            Prim::Seg(p, q),
        ) => {
            let d = sub(q, p);
            let f = sub(p, center);
            let qa = dot2(d, d);
            if qa <= 0.0 {
                return out;
            }
            let qb = 2.0 * dot2(f, d);
            let qc = dot2(f, f) - radius * radius;
            let disc = qb * qb - 4.0 * qa * qc;
            if disc < 0.0 {
                return out;
            }
            let root = disc.sqrt();
            let eb = slack_of(b);
            let ang_slack = tol / radius.max(tol);
            for v in [(-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa)] {
                if v < -eb || v > 1.0 + eb {
                    continue;
                }
                let x = lerp(p, q, v);
                let angle = (x[1] - center[1]).atan2(x[0] - center[0]);
                if let Some(u) = Prim::arc_parameter(start, sweep, angle, ang_slack) {
                    out.push((u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)));
                }
                if root <= 0.0 {
                    break;
                }
            }
        }
        (
            Prim::Arc {
                center: c1,
                radius: r1,
                start: s1,
                sweep: w1,
            },
            Prim::Arc {
                center: c2,
                radius: r2,
                start: s2,
                sweep: w2,
            },
        ) => {
            let d = dist(c1, c2);
            if d <= 1e-12 || d > r1 + r2 + tol || d < (r1 - r2).abs() - tol {
                return out;
            }
            let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
            let h = (r1 * r1 - a * a).max(0.0).sqrt();
            let e = scale(sub(c2, c1), 1.0 / d);
            let m = add(c1, scale(e, a));
            let mut points = vec![add(m, scale([-e[1], e[0]], h))];
            if h > 0.0 {
                points.push(add(m, scale([e[1], -e[0]], h)));
            }
            for x in points {
                let t1 = Prim::arc_parameter(
                    s1,
                    w1,
                    (x[1] - c1[1]).atan2(x[0] - c1[0]),
                    tol / r1.max(tol),
                );
                let t2 = Prim::arc_parameter(
                    s2,
                    w2,
                    (x[1] - c2[1]).atan2(x[0] - c2[0]),
                    tol / r2.max(tol),
                );
                if let (Some(t1), Some(t2)) = (t1, t2) {
                    out.push((t1.clamp(0.0, 1.0), t2.clamp(0.0, 1.0)));
                }
            }
        }
    }
    out
}

impl Piece {
    /// How far a point lies from it (mm): from a line or an arc exactly,
    /// from a polyline's segments, from a B-spline's points within
    /// [`SPLINE_SAMPLES`].
    pub fn distance(&self, p: P2) -> f64 {
        self.prims()
            .iter()
            .map(|(prim, _, _)| prim.nearest(p).1)
            .fold(f64::INFINITY, f64::min)
    }

    /// Its points from its start to its end, arcs in steps of at most 2
    /// degrees.
    pub fn flattened(&self) -> Vec<P2> {
        let mut out = vec![self.start()];
        self.flatten_into(&mut out);
        out
    }

    /// Its lines and arcs, each with the parameter it starts at and the
    /// span of the piece's parameter it covers.
    fn prims(&self) -> Vec<(Prim, f64, f64)> {
        match self {
            Self::Line(a, b) => vec![(Prim::Seg(*a, *b), 0.0, 1.0)],
            Self::Arc {
                center,
                radius,
                start,
                sweep,
            } => vec![(
                Prim::Arc {
                    center: *center,
                    radius: *radius,
                    start: *start,
                    sweep: *sweep,
                },
                0.0,
                1.0,
            )],
            Self::Polyline(points) => points
                .windows(2)
                .enumerate()
                .map(|(i, w)| (Prim::Seg(w[0], w[1]), i as f64, 1.0))
                .collect(),
            Self::Spline { curve, from, to } => {
                let span = to - from;
                let t = |u: f64| if span != 0.0 { (u - from) / span } else { 0.0 };
                curve
                    .points(*from, *to)
                    .windows(2)
                    .map(|w| (Prim::Seg(w[0].1, w[1].1), t(w[0].0), t(w[1].0) - t(w[0].0)))
                    .collect()
            }
        }
    }

    fn bounds(&self) -> (P2, P2) {
        let mut points = vec![self.start()];
        self.flatten_into(&mut points);
        let mut lo = points[0];
        let mut hi = points[0];
        for p in points {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        // An arc's sagitta between its flattened points, a B-spline's
        // distance from its points.
        let pad = match self {
            Self::Arc { radius, .. } => radius * (1.0 - (1f64.to_radians()).cos()),
            Self::Spline { .. } => SPLINE_SAMPLES,
            _ => 0.0,
        };
        lo = [lo[0] - pad, lo[1] - pad];
        hi = [hi[0] + pad, hi[1] + pad];
        (lo, hi)
    }

    /// The parameter of the point nearest `p` and its distance.
    fn nearest(&self, p: P2) -> (f64, f64) {
        let (t, d) = self
            .prims()
            .iter()
            .map(|(prim, t0, span)| {
                let (t, d) = prim.nearest(p);
                (t0 + t * span, d)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((0.0, f64::INFINITY));
        // On a B-spline its points are only near it: the curve itself.
        if matches!(self, Self::Spline { .. }) && d <= 2.0 * SPLINE_SAMPLES {
            let t = self.refined_nearest(p, t);
            return (t, dist(self.point(t), p));
        }
        (t, d)
    }

    /// The pieces between the parameters given (in any order).
    fn split(&self, mut at: Vec<f64>, tol: f64) -> Vec<Piece> {
        let end = self.domain();
        at.sort_by(f64::total_cmp);
        let mut cuts: Vec<f64> = Vec::new();
        let mut last = 0.0;
        for t in at {
            let p = self.point(t);
            if dist(p, self.point(last)) > tol && dist(p, self.end()) > tol && t > last {
                cuts.push(t);
                last = t;
            }
        }
        if cuts.is_empty() {
            return vec![self.clone()];
        }
        let mut out = Vec::with_capacity(cuts.len() + 1);
        let mut from = 0.0;
        for t in cuts {
            out.push(self.slice(from, t));
            from = t;
        }
        out.push(self.slice(from, end));
        out
    }
}

/// A projection's polyline without repeated points, if it has two.
fn polyline_of(points: &[P2]) -> Option<Piece> {
    let mut kept: Vec<P2> = Vec::with_capacity(points.len());
    for p in points {
        if kept.last().is_none_or(|q| dist(*q, *p) > 0.0) {
            kept.push(*p);
        }
    }
    (kept.len() >= 2).then_some(Piece::Polyline(kept))
}

/// The pieces of the lines a projection sees.
fn pieces_of(edges: &[ViewEdge]) -> Vec<Piece> {
    let mut out = Vec::new();
    for edge in edges.iter().filter(|e| e.visible) {
        match &edge.curve {
            ViewCurve::Line(a, b) => out.push(Piece::Line(*a, *b)),
            ViewCurve::Arc {
                center,
                radius,
                start,
                end,
            } => out.push(Piece::Arc {
                center: *center,
                radius: *radius,
                start: *start,
                sweep: end - start,
            }),
            ViewCurve::Polyline(points) => {
                if let Some(piece) = polyline_of(points) {
                    out.push(piece);
                }
            }
            // A B-spline as itself (mitcad#853); its polyline when its data
            // is no B-spline this crate can cut.
            ViewCurve::Spline(view) => match Spline2::of(view) {
                Some(spline) => {
                    let (from, to) = spline.domain();
                    out.push(Piece::Spline {
                        curve: Arc::new(SplineCurve::new(spline)),
                        from,
                        to,
                    });
                }
                None => {
                    if let Some(piece) = polyline_of(&view.points) {
                        out.push(piece);
                    }
                }
            },
        }
    }
    out
}

/// Points merged within a distance, found through a grid.
struct Nodes {
    points: Vec<P2>,
    cells: HashMap<(i64, i64), Vec<usize>>,
    cell: f64,
    tol: f64,
}

impl Nodes {
    fn new(tol: f64) -> Self {
        Self {
            points: Vec::new(),
            cells: HashMap::new(),
            cell: tol * 4.0,
            tol,
        }
    }

    fn key(&self, p: P2) -> (i64, i64) {
        (
            (p[0] / self.cell).floor() as i64,
            (p[1] / self.cell).floor() as i64,
        )
    }

    fn node(&mut self, p: P2) -> usize {
        let (kx, ky) = self.key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(found) = self.cells.get(&(kx + dx, ky + dy)) {
                    for &i in found {
                        if dist(self.points[i], p) <= self.tol {
                            return i;
                        }
                    }
                }
            }
        }
        let i = self.points.len();
        self.points.push(p);
        self.cells.entry((kx, ky)).or_default().push(i);
        i
    }
}

/// One side of an edge of the arrangement: from `from` along `piece`.
struct HalfEdge {
    from: usize,
    to: usize,
    piece: Piece,
}

/// The outline of what a projection sees: the lines that bound the
/// shadow of the body along the view (see the module's documentation).
/// `material` says for points of the view's plane whether the body lies
/// along the line of sight through each of them.
pub fn silhouette(
    edges: &[ViewEdge],
    mut material: impl FnMut(&[P2]) -> Result<Vec<bool>, String>,
) -> Result<Outline, String> {
    let pieces = pieces_of(edges);
    // The tolerance of the projection's ends: a micrometre on parts up to
    // 100 mm, more on larger ones.
    let mut extent = 0.0f64;
    for p in &pieces {
        let (lo, hi) = p.bounds();
        extent = extent.max(hi[0] - lo[0]).max(hi[1] - lo[1]);
    }
    let tol = 1e-5 * (extent / 100.0).max(1.0);
    let pieces: Vec<Piece> = pieces.into_iter().filter(|p| p.length() > tol).collect();
    if pieces.is_empty() {
        return Ok(Outline::default());
    }

    // Split every piece where another one crosses it or ends on it.
    let boxes: Vec<(P2, P2)> = pieces.iter().map(Piece::bounds).collect();
    let prims: Vec<Vec<(Prim, f64, f64)>> = pieces.iter().map(Piece::prims).collect();
    let mut splits: Vec<Vec<f64>> = vec![Vec::new(); pieces.len()];
    let near = |a: &(P2, P2), b: &(P2, P2)| {
        a.0[0] <= b.1[0] + tol
            && b.0[0] <= a.1[0] + tol
            && a.0[1] <= b.1[1] + tol
            && b.0[1] <= a.1[1] + tol
    };
    for i in 0..pieces.len() {
        for j in (i + 1)..pieces.len() {
            if !near(&boxes[i], &boxes[j]) {
                continue;
            }
            for (pa, ta, sa) in &prims[i] {
                for (pb, tb, sb) in &prims[j] {
                    for (u, v) in crossings(pa, pb, tol) {
                        let (ti, tj) =
                            refined_crossing(&pieces[i], &pieces[j], ta + u * sa, tb + v * sb);
                        splits[i].push(ti);
                        splits[j].push(tj);
                    }
                }
            }
            // The ends of each on the other (where one meets the other
            // without crossing it, or lies along it).
            for (k, l) in [(i, j), (j, i)] {
                for end in [pieces[k].start(), pieces[k].end()] {
                    let (t, d) = pieces[l].nearest(end);
                    if d <= tol {
                        splits[l].push(t);
                    }
                }
            }
        }
    }
    let mut split: Vec<Piece> = Vec::new();
    for (piece, at) in pieces.iter().zip(splits) {
        for part in piece.split(at, tol) {
            // A closed piece (a whole circle) in two halves, so that every
            // edge has two ends.
            if dist(part.start(), part.end()) <= tol {
                if part.length() > 4.0 * tol {
                    let half = part.domain() / 2.0;
                    split.push(part.slice(0.0, half));
                    split.push(part.slice(half, part.domain()));
                }
            } else {
                split.push(part);
            }
        }
    }

    // The edges between merged ends, each once.
    let mut nodes = Nodes::new(tol);
    let mut edges_of: Vec<(usize, usize, Piece)> = Vec::new();
    let mut seen: HashMap<(usize, usize), Vec<P2>> = HashMap::new();
    for piece in split {
        let u = nodes.node(piece.start());
        let v = nodes.node(piece.end());
        if u == v {
            continue;
        }
        let piece = match piece {
            Piece::Line(..) => Piece::Line(nodes.points[u], nodes.points[v]),
            Piece::Polyline(mut points) => {
                points[0] = nodes.points[u];
                let last = points.len() - 1;
                points[last] = nodes.points[v];
                Piece::Polyline(points)
            }
            arc => arc,
        };
        let middle = piece.point(piece.domain() / 2.0);
        let key = (u.min(v), u.max(v));
        let middles = seen.entry(key).or_default();
        if middles.iter().any(|m| dist(*m, middle) <= 10.0 * tol) {
            continue;
        }
        middles.push(middle);
        edges_of.push((u, v, piece));
    }
    if edges_of.is_empty() {
        return Ok(Outline::default());
    }

    // Half-edges 2k (forward) and 2k + 1 (back), sorted about each node
    // by the way they leave it.
    let mut half: Vec<HalfEdge> = Vec::with_capacity(edges_of.len() * 2);
    for (u, v, piece) in edges_of {
        let back = piece.reversed();
        half.push(HalfEdge {
            from: u,
            to: v,
            piece,
        });
        half.push(HalfEdge {
            from: v,
            to: u,
            piece: back,
        });
    }
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); nodes.points.len()];
    for (h, e) in half.iter().enumerate() {
        out[e.from].push(h);
    }
    let mut position = vec![0usize; half.len()];
    for (node, list) in out.iter_mut().enumerate() {
        // The same short distance along each, so that lines and arcs that
        // leave the node tangent to each other are told apart.
        let reach = list
            .iter()
            .map(|h| half[*h].piece.length() / 3.0)
            .fold(0.5, f64::min);
        let origin = nodes.points[node];
        let mut angles: Vec<(f64, usize)> = list
            .iter()
            .map(|h| {
                let p = half[*h].piece.at_distance(reach);
                ((p[1] - origin[1]).atan2(p[0] - origin[0]), *h)
            })
            .collect();
        angles.sort_by(|a, b| a.0.total_cmp(&b.0));
        *list = angles.into_iter().map(|(_, h)| h).collect();
        for (i, h) in list.iter().enumerate() {
            position[*h] = i;
        }
    }
    // The half-edge after another round the region on its left: at its
    // end, the next one clockwise from the way back.
    let next = |h: usize| {
        let list = &out[half[h].to];
        let back = h ^ 1;
        list[(position[back] + list.len() - 1) % list.len()]
    };

    // The regions' boundaries, each asked once whether material lies
    // behind it, at a point just left of its longest half-edge.
    let mut cycle = vec![usize::MAX; half.len()];
    let mut probes: Vec<P2> = Vec::new();
    for h in 0..half.len() {
        if cycle[h] != usize::MAX {
            continue;
        }
        let id = probes.len();
        let mut longest = (h, 0.0);
        let mut at = h;
        for _ in 0..half.len() {
            cycle[at] = id;
            let l = half[at].piece.length();
            if l > longest.1 {
                longest = (at, l);
            }
            at = next(at);
            if at == h || cycle[at] != usize::MAX {
                break;
            }
        }
        let (m, d) = half[longest.0].piece.middle();
        let step = (1e-3 * (extent / 1000.0).max(1.0))
            .max(20.0 * tol)
            .min(longest.1 / 4.0);
        probes.push(add(m, scale([-d[1], d[0]], step)));
    }
    let solid = material(&probes)?;
    if solid.len() != probes.len() {
        return Err(format!(
            "{} points classified for {} asked",
            solid.len(),
            probes.len()
        ));
    }

    // The cuts: material on their left only, followed into loops.
    let kept = |h: usize| solid[cycle[h]] && !solid[cycle[h ^ 1]];
    let next_kept = |h: usize| -> Option<usize> {
        let list = &out[half[h].to];
        let back = position[h ^ 1];
        (1..=list.len())
            .map(|k| list[(back + list.len() * 2 - k) % list.len()])
            .find(|c| kept(*c))
    };
    let mut used = vec![false; half.len()];
    let mut loops: Vec<OutlineLoop> = Vec::new();
    for h in 0..half.len() {
        if used[h] || !kept(h) {
            continue;
        }
        let mut pieces: Vec<Piece> = Vec::new();
        let mut at = h;
        let mut closed = false;
        for _ in 0..half.len() {
            used[at] = true;
            pieces.push(half[at].piece.clone());
            match next_kept(at) {
                Some(n) if n == h => {
                    closed = true;
                    break;
                }
                Some(n) if !used[n] => at = n,
                _ => break,
            }
        }
        if !closed {
            continue;
        }
        let mut l = OutlineLoop {
            outer: false,
            pieces: joined(pieces),
        };
        let area = l.signed_area();
        if area.abs() <= tol * tol {
            continue;
        }
        l.outer = area > 0.0;
        loops.push(l);
    }
    loops.sort_by(|a, b| {
        b.outer
            .cmp(&a.outer)
            .then(b.signed_area().abs().total_cmp(&a.signed_area().abs()))
    });
    Ok(Outline { loops })
}

/// One piece of two that follow each other on one line, one circle or as
/// polylines, or None.
fn join(a: &Piece, b: &Piece) -> Option<Piece> {
    match (a, b) {
        (Piece::Line(p, q), Piece::Line(q2, r)) => {
            let (u, v) = (sub(*q, *p), sub(*r, *q2));
            let (lu, lv) = (norm(u), norm(v));
            (dist(*q, *q2) <= 1e-9
                && lu > 0.0
                && lv > 0.0
                && cross2(u, v).abs() <= 1e-9 * lu * lv
                && dot2(u, v) > 0.0)
                .then_some(Piece::Line(*p, *r))
        }
        (
            Piece::Arc {
                center: c1,
                radius: r1,
                start: s1,
                sweep: w1,
            },
            Piece::Arc {
                center: c2,
                radius: r2,
                start: _,
                sweep: w2,
            },
        ) => {
            let close = 1e-9 * r1.max(1.0);
            (dist(*c1, *c2) <= close
                && (r1 - r2).abs() <= close
                && (*w1 > 0.0) == (*w2 > 0.0)
                && dist(a.end(), b.start()) <= 1e-6
                && (w1 + w2).abs() <= TAU + 1e-9)
                .then(|| {
                    let sweep = w1 + w2;
                    Piece::Arc {
                        center: *c1,
                        radius: *r1,
                        start: *s1,
                        sweep: if (sweep.abs() - TAU).abs() <= 1e-9 {
                            TAU * sweep.signum()
                        } else {
                            sweep
                        },
                    }
                })
        }
        (Piece::Polyline(p), Piece::Polyline(q)) => {
            let mut points = p.clone();
            points.extend(q.iter().skip(1));
            Some(Piece::Polyline(points))
        }
        // Two pieces of one B-spline that follow each other on it.
        (
            Piece::Spline {
                curve: c1,
                from: f1,
                to: t1,
            },
            Piece::Spline {
                curve: c2,
                from: f2,
                to: t2,
            },
        ) => {
            let (lo, hi) = c1.spline.domain();
            let close = 1e-12 * (hi - lo).abs().max(1.0);
            (Arc::ptr_eq(c1, c2) && (t1 - f2).abs() <= close && (t1 > f1) == (t2 > f2)).then(|| {
                Piece::Spline {
                    curve: Arc::clone(c1),
                    from: *f1,
                    to: *t2,
                }
            })
        }
        _ => None,
    }
}

/// A loop's pieces with those that follow each other on one line or one
/// circle as one, the last with the first too.
fn joined(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        match out.last().and_then(|last| join(last, &piece)) {
            Some(both) => *out.last_mut().expect("a last piece") = both,
            None => out.push(piece),
        }
    }
    while out.len() > 1 {
        let Some(both) = join(out.last().expect("a last piece"), &out[0]) else {
            break;
        };
        out[0] = both;
        out.pop();
    }
    out
}

// --- Parts on the sheet -----------------------------------------------------

/// The middle of an outline's box.
pub fn middle(outline: &Outline) -> P2 {
    outline
        .bounds()
        .map_or([0.0, 0.0], |(lo, hi)| lerp(lo, hi, 0.5))
}

/// An outline on the sheet: the middle of its box at `place`, turned by
/// `degrees` counter-clockwise about it.
pub fn placed(outline: &Outline, place: P2, degrees: f64) -> Vec<OutlineLoop> {
    let mid = middle(outline);
    let angle = degrees.to_radians();
    let (s, c) = angle.sin_cos();
    // Turned about the origin after moving the middle there: the middle's
    // turned place is subtracted from `place`.
    let by = sub(place, [c * mid[0] - s * mid[1], s * mid[0] + c * mid[1]]);
    outline
        .loops
        .iter()
        .map(|l| OutlineLoop {
            outer: l.outer,
            pieces: l.pieces.iter().map(|p| p.moved(angle, by)).collect(),
        })
        .collect()
}

/// A Flat Pattern part's bend lines on the sheet (mitcad#1980), moved and
/// turned with its outline as [`placed`] places it.
pub fn placed_bends(view: &PartView, place: P2, degrees: f64) -> Vec<BendLine> {
    let mid = middle(&view.outline);
    let angle = degrees.to_radians();
    let (s, c) = angle.sin_cos();
    let by = sub(place, [c * mid[0] - s * mid[1], s * mid[0] + c * mid[1]]);
    let f = |p: P2| [c * p[0] - s * p[1] + by[0], s * p[0] + c * p[1] + by[1]];
    view.bends
        .iter()
        .map(|b| BendLine {
            start: f(b.start),
            end: f(b.end),
            up: b.up,
        })
        .collect()
}

/// A part's marks on the sheet (mitcad#1939), moved and turned with its
/// outline as [`placed`] places it.
pub fn placed_marks(view: &PartView, marks: &[Mark], place: P2, degrees: f64) -> Vec<Mark> {
    let mid = middle(&view.outline);
    let angle = degrees.to_radians();
    let (s, c) = angle.sin_cos();
    let by = sub(place, [c * mid[0] - s * mid[1], s * mid[0] + c * mid[1]]);
    marks
        .iter()
        .map(|m| Mark {
            kind: m.kind,
            piece: m.piece.moved(angle, by),
        })
        .collect()
}

/// A mark on a part (mitcad#1939): a piece of a sketch's curve laid on
/// the part, engraved or scored.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub kind: MarkKind,
    pub piece: Piece,
}

/// A part laid out on the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedPart {
    pub id: u32,
    /// Its body's name.
    pub name: String,
    /// Sheet millimetres.
    pub loops: Vec<OutlineLoop>,
    /// A Flat Pattern part's bend lines (mitcad#1980), sheet millimetres.
    pub bends: Vec<BendLine>,
    /// Its engraved and scored marks (mitcad#1939), sheet millimetres.
    pub marks: Vec<Mark>,
    /// Its sheet's process profile, whose output preset names the layers
    /// of its bend lines and marks (mitcad#1939); metal for a part that is
    /// no sheet.
    pub process: SheetProcess,
}

/// Whether loops reach out of a sheet of this size.
pub fn off_sheet(loops: &[OutlineLoop], width: f64, height: f64) -> bool {
    const SLACK: f64 = 1e-6;
    bounds(loops).is_some_and(|(lo, hi)| {
        lo[0] < -SLACK || lo[1] < -SLACK || hi[0] > width + SLACK || hi[1] > height + SLACK
    })
}

/// Whether a point lies in the material the loops bound (inside an odd
/// number of them, counting outer boundaries and holes alike).
fn in_material(polygons: &[Vec<P2>], p: P2) -> bool {
    let mut inside = false;
    for polygon in polygons {
        let n = polygon.len();
        for i in 0..n {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if p[0] < x {
                    inside = !inside;
                }
            }
        }
    }
    inside
}

/// Points just inside the material along a loop: left of the middle of
/// each of its segments (the material lies left of every loop).
fn inside_points(polygon: &[P2]) -> Vec<P2> {
    let n = polygon.len();
    (0..n)
        .filter_map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            let l = dist(a, b);
            (l > 0.0).then(|| {
                let d = scale(sub(b, a), 1.0 / l);
                add(lerp(a, b, 0.5), scale([-d[1], d[0]], (l / 4.0).min(1e-3)))
            })
        })
        .collect()
}

/// Whether two parts' material overlaps: their boundaries cross, or the
/// material just inside one's boundary lies in the other's (touching is
/// no overlap).
pub fn overlap(a: &[OutlineLoop], b: &[OutlineLoop]) -> bool {
    let (Some(ba), Some(bb)) = (bounds(a), bounds(b)) else {
        return false;
    };
    if ba.1[0] <= bb.0[0] || bb.1[0] <= ba.0[0] || ba.1[1] <= bb.0[1] || bb.1[1] <= ba.0[1] {
        return false;
    }
    let pa: Vec<Vec<P2>> = a.iter().map(OutlineLoop::polygon).collect();
    let pb: Vec<Vec<P2>> = b.iter().map(OutlineLoop::polygon).collect();
    const E: f64 = 1e-9;
    for p in &pa {
        for q in &pb {
            for i in 0..p.len() {
                let (p0, p1) = (p[i], p[(i + 1) % p.len()]);
                for j in 0..q.len() {
                    let (q0, q1) = (q[j], q[(j + 1) % q.len()]);
                    let d1 = sub(p1, p0);
                    let d2 = sub(q1, q0);
                    let den = cross2(d1, d2);
                    if den.abs() <= 1e-12 * norm(d1) * norm(d2) {
                        continue;
                    }
                    let w = sub(q0, p0);
                    let u = cross2(w, d2) / den;
                    let v = cross2(w, d1) / den;
                    if u > E && u < 1.0 - E && v > E && v < 1.0 - E {
                        return true;
                    }
                }
            }
        }
    }
    let inside = |polygons: &[Vec<P2>], other: &[Vec<P2>]| {
        polygons
            .iter()
            .flat_map(|p| inside_points(p.as_slice()))
            .any(|p| in_material(other, p))
    };
    inside(&pa, &pb) || inside(&pb, &pa)
}

/// The height of a part's name on the sheet: 5 mm, smaller on small parts.
fn label_height(loops: &[OutlineLoop]) -> f64 {
    bounds(loops).map_or(5.0, |(lo, hi)| {
        (0.25 * (hi[0] - lo[0]).min(hi[1] - lo[1])).clamp(1.0, 5.0)
    })
}

fn label_at(loops: &[OutlineLoop]) -> P2 {
    bounds(loops).map_or([0.0, 0.0], |(lo, hi)| lerp(lo, hi, 0.5))
}

/// A piece as an item of a drawn sheet.
fn item(pen: Pen, piece: &Piece) -> Item {
    match piece {
        Piece::Line(a, b) => Item::Line { pen, a: *a, b: *b },
        Piece::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            let (from, span) = if *sweep >= 0.0 {
                (*start, *sweep)
            } else {
                (start + sweep, -sweep)
            };
            let from = from.rem_euclid(TAU);
            Item::Arc {
                pen,
                center: *center,
                radius: *radius,
                start: from,
                end: from + span,
            }
        }
        Piece::Polyline(points) => Item::Polyline {
            pen,
            points: points.clone(),
            closed: false,
        },
        Piece::Spline { .. } => {
            let mut points = vec![piece.start()];
            piece.flatten_into(&mut points);
            Item::Polyline {
                pen,
                points,
                closed: false,
            }
        }
    }
}

/// The parts as a sheet the application draws (the `drawing` query's
/// groups): each part a `view` group `part-<id>` of its loops, its name
/// as a `view-label` group.
pub fn sheet(width: f64, height: f64, title: &str, parts: &[PlacedPart]) -> Sheet {
    let mut groups = Vec::new();
    for part in parts {
        let mut group = Group::new("view");
        group.id = Some(format!("part-{}", part.id));
        for l in &part.loops {
            for piece in &l.pieces {
                group.items.push(item(Pen::Thick, piece));
            }
        }
        // A Flat Pattern's bend lines (mitcad#1980).
        for bend in &part.bends {
            group.items.push(Item::Line {
                pen: Pen::Chain,
                a: bend.start,
                b: bend.end,
            });
        }
        groups.push(group);
        let mut label = Group::new("view-label");
        label.id = Some(format!("part-{}-label", part.id));
        label.text(
            label_at(&part.loops),
            label_height(&part.loops).min(3.5),
            0.0,
            Anchor::Middle,
            part.name.clone(),
        );
        groups.push(label);
    }
    Sheet {
        width,
        height,
        title: title.to_owned(),
        groups,
    }
}

// --- The files --------------------------------------------------------------

/// The parts as a DXF drawing in millimetres at 1:1: outer boundaries on
/// `CUT_OUTER`, inner loops on `CUT_INNER`, lines, arcs and circles as
/// such, polylines as splines of degree 1; with `labels` each part's name
/// on `LABEL` at the middle of its box.
pub fn dxf(parts: &[PlacedPart], labels: bool) -> mitcad_dxf::Drawing {
    use mitcad_dxf::{Drawing, Geometry, Layer, Point2, Spline, Text, Units};
    let mut out = Drawing::new(Units::Millimeters);
    let mut layers: Vec<(String, i16)> = vec![(CUT_OUTER.into(), 7), (CUT_INNER.into(), 6)];
    let mut layer = |name: &str, color: i16| {
        if !layers.iter().any(|(n, _)| n == name) {
            layers.push((name.to_owned(), color));
        }
    };
    // A Flat Pattern's bend lines on its layers (mitcad#1980), the part's
    // marks on its engrave and score layers, by its process profile
    // (mitcad#1939).
    for part in parts {
        let preset = OutputPreset::of(part.process);
        for bend in &part.bends {
            let name = if bend.up {
                &preset.bend_up
            } else {
                &preset.bend_down
            };
            layer(name, preset.colour(name));
        }
        for mark in &part.marks {
            match mark.kind {
                MarkKind::Engrave => layer(&preset.engrave, preset.colour(&preset.engrave)),
                MarkKind::Score => layer(&preset.score, preset.colour(&preset.score)),
                MarkKind::Cut => {}
            }
        }
    }
    if labels {
        layer(LABEL, 3);
    }
    out.layers = layers
        .into_iter()
        .map(|(name, color)| Layer {
            color,
            ..Layer::new(name)
        })
        .collect();
    let p = |q: P2| Point2::new(q[0], q[1]);
    let geometry_of = |piece: &Piece| -> Geometry {
        match piece {
            Piece::Line(a, b) => Geometry::Line {
                start: p(*a),
                end: p(*b),
            },
            Piece::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                if sweep.abs() >= TAU - 1e-9 {
                    Geometry::Circle {
                        center: p(*center),
                        radius: *radius,
                    }
                } else if *sweep >= 0.0 {
                    Geometry::arc(p(*center), *radius, *start, start + sweep)
                } else {
                    Geometry::arc(p(*center), *radius, start + sweep, *start)
                }
            }
            // The B-spline as it is: its degree, knots, poles and
            // weights (mitcad#853).
            Piece::Spline { .. } => {
                let exact = piece.exact().expect("a B-spline");
                Geometry::Spline(Spline {
                    degree: exact.degree as u32,
                    control_points: exact.poles.iter().map(|q| p(*q)).collect(),
                    weights: if exact.is_rational() {
                        exact.weights.clone()
                    } else {
                        Vec::new()
                    },
                    knots: exact.knots,
                    fit_points: Vec::new(),
                    closed: false,
                    periodic: false,
                })
            }
            Piece::Polyline(points) => {
                // Clamped, degree 1: the polyline itself.
                let n = points.len();
                let mut knots = vec![0.0];
                knots.extend((0..n).map(|i| i as f64));
                knots.push((n - 1) as f64);
                Geometry::Spline(Spline {
                    degree: 1,
                    control_points: points.iter().map(|q| p(*q)).collect(),
                    knots,
                    weights: Vec::new(),
                    fit_points: Vec::new(),
                    closed: false,
                    periodic: false,
                })
            }
        }
    };
    for part in parts {
        for l in &part.loops {
            let layer = if l.outer { CUT_OUTER } else { CUT_INNER };
            for piece in &l.pieces {
                out.push(layer, geometry_of(piece));
            }
        }
        // A Flat Pattern's bend lines and the part's marks on the layers
        // of its process profile's output preset (mitcad#1939).
        let preset = OutputPreset::of(part.process);
        for bend in &part.bends {
            out.push(
                if bend.up {
                    preset.bend_up.as_str()
                } else {
                    preset.bend_down.as_str()
                },
                Geometry::Line {
                    start: p(bend.start),
                    end: p(bend.end),
                },
            );
        }
        for mark in &part.marks {
            let layer = match mark.kind {
                MarkKind::Engrave => preset.engrave.as_str(),
                MarkKind::Score => preset.score.as_str(),
                MarkKind::Cut => CUT_INNER,
            };
            out.push(layer, geometry_of(&mark.piece));
        }
        if labels {
            let height = label_height(&part.loops);
            let at = label_at(&part.loops);
            let shift = text_width(&part.name, height) / 2.0;
            out.push(
                LABEL,
                Geometry::Text(Text {
                    position: p([at[0] - shift, at[1] - height / 2.0]),
                    height,
                    rotation: 0.0,
                    content: part.name.clone(),
                }),
            );
        }
    }
    out
}

fn n(v: f64) -> String {
    trim_number(v, 6)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// A loop as an SVG path's data, y up turned to SVG's y down.
fn path_data(l: &OutlineLoop, height: f64) -> String {
    let at = |q: P2| format!("{} {}", n(q[0]), n(height - q[1]));
    let mut d = String::new();
    let Some(first) = l.pieces.first() else {
        return d;
    };
    let _ = write!(d, "M {}", at(first.start()));
    for piece in &l.pieces {
        match piece {
            Piece::Line(_, b) => {
                let _ = write!(d, " L {}", at(*b));
            }
            Piece::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                // A counter-clockwise arc on the sheet runs clockwise in
                // SVG's coordinates (sweep flag 0); a whole circle in two.
                let halves = if sweep.abs() > PI + 1e-9 { 2 } else { 1 };
                let step = sweep / halves as f64;
                for k in 1..=halves {
                    let to = polar(*center, *radius, start + step * k as f64);
                    let large = u8::from(step.abs() > PI);
                    let flag = u8::from(*sweep < 0.0);
                    let _ = write!(
                        d,
                        " A {} {} 0 {large} {flag} {}",
                        n(*radius),
                        n(*radius),
                        at(to)
                    );
                }
            }
            Piece::Polyline(points) => {
                for q in points.iter().skip(1) {
                    let _ = write!(d, " L {}", at(*q));
                }
            }
            // A B-spline of degree 3 or less as its Bézier pieces (cubic
            // Béziers, exactly); another as lines within SVG_DEFLECTION.
            Piece::Spline { .. } => {
                let exact = piece.exact().expect("a B-spline");
                match exact.beziers() {
                    Some(pieces) => {
                        for b in pieces {
                            if let [_, end] = b.as_slice() {
                                let _ = write!(d, " L {}", at(*end));
                            } else {
                                let c = bspline::cubic(&b);
                                let _ = write!(d, " C {} {} {}", at(c[1]), at(c[2]), at(c[3]));
                            }
                        }
                    }
                    None => {
                        let (a, b) = exact.domain();
                        for (_, q) in exact.samples(a, b, SVG_DEFLECTION).iter().skip(1) {
                            let _ = write!(d, " L {}", at(*q));
                        }
                    }
                }
            }
        }
    }
    d.push_str(" Z");
    d
}

/// The parts as an SVG drawing of the material's size in millimetres at
/// 1:1: a layer (`<g>`) `CUT_OUTER` of the outer boundaries and
/// `CUT_INNER` of the inner loops, each loop one closed path; with
/// `labels` a layer `LABEL` of the parts' names.
pub fn svg(parts: &[PlacedPart], width: f64, height: f64, labels: bool) -> String {
    let mut s = String::new();
    let _ = writeln!(s, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" width="{w}mm" height="{h}mm" viewBox="0 0 {w} {h}">"#,
        w = n(width),
        h = n(height)
    );
    for (layer, outer, colour) in [(CUT_OUTER, true, "#ff0000"), (CUT_INNER, false, "#0000ff")] {
        let _ = writeln!(
            s,
            r#"<g id="{layer}" inkscape:groupmode="layer" inkscape:label="{layer}" fill="none" stroke="{colour}" stroke-width="0.1">"#
        );
        for part in parts {
            for l in part.loops.iter().filter(|l| l.outer == outer) {
                let _ = writeln!(
                    s,
                    r#"<path data-part="{}" d="{}"/>"#,
                    part.id,
                    path_data(l, height)
                );
            }
        }
        let _ = writeln!(s, "</g>");
    }
    // A Flat Pattern's bend lines (mitcad#1980), each an open path.
    if parts.iter().any(|p| !p.bends.is_empty()) {
        for (layer, up, colour) in [(BEND_UP, true, "#00a000"), (BEND_DOWN, false, "#a000a0")] {
            let _ = writeln!(
                s,
                r#"<g id="{layer}" inkscape:groupmode="layer" inkscape:label="{layer}" fill="none" stroke="{colour}" stroke-width="0.1">"#
            );
            for part in parts {
                for bend in part.bends.iter().filter(|b| b.up == up) {
                    let _ = writeln!(
                        s,
                        r#"<path data-part="{}" d="M {} {} L {} {}"/>"#,
                        part.id,
                        n(bend.start[0]),
                        n(height - bend.start[1]),
                        n(bend.end[0]),
                        n(height - bend.end[1])
                    );
                }
            }
            let _ = writeln!(s, "</g>");
        }
    }
    // The parts' marks (mitcad#1939): a layer per kind, each mark an open
    // path; a kerf bend's slits with the cuts' colour.
    for (kind, name, colour) in [
        (MarkKind::Cut, "SLITS", "#ff00ff"),
        (MarkKind::Score, "SCORE", "#0000ff"),
        (MarkKind::Engrave, "ENGRAVE", "#ffa000"),
    ] {
        if !parts.iter().any(|p| p.marks.iter().any(|m| m.kind == kind)) {
            continue;
        }
        let _ = writeln!(
            s,
            r#"<g id="{name}" inkscape:groupmode="layer" inkscape:label="{name}" fill="none" stroke="{colour}" stroke-width="0.1">"#
        );
        for part in parts {
            for mark in part.marks.iter().filter(|m| m.kind == kind) {
                let mut points = vec![mark.piece.start()];
                mark.piece.flatten_into(&mut points);
                let d: Vec<String> = points
                    .iter()
                    .enumerate()
                    .map(|(i, q)| {
                        format!(
                            "{} {} {}",
                            if i == 0 { "M" } else { "L" },
                            n(q[0]),
                            n(height - q[1])
                        )
                    })
                    .collect();
                let _ = writeln!(s, r#"<path data-part="{}" d="{}"/>"#, part.id, d.join(" "));
            }
        }
        let _ = writeln!(s, "</g>");
    }
    if labels {
        let _ = writeln!(
            s,
            r##"<g id="{LABEL}" inkscape:groupmode="layer" inkscape:label="{LABEL}" fill="#000000" font-family="Helvetica, Arial, sans-serif">"##
        );
        for part in parts {
            let h = label_height(&part.loops);
            let at = label_at(&part.loops);
            let _ = writeln!(
                s,
                r#"<text data-part="{}" x="{}" y="{}" font-size="{}" text-anchor="middle">{}</text>"#,
                part.id,
                n(at[0]),
                n(height - at[1] + h / 2.0),
                n(super::sheet::font_size(h)),
                escape(&part.name)
            );
        }
        let _ = writeln!(s, "</g>");
    }
    let _ = writeln!(s, "</svg>");
    s
}

#[cfg(test)]
#[path = "cut_sheet_tests.rs"]
mod tests;

// --- Kerf compensation and nesting (mitcad#1939) ------------------------------

/// How far apart the ends of two moved pieces may lie and still meet
/// (mm): pieces that run on from one another at a tangent.
const KERF_JOINT: f64 = 1e-6;

/// The loops with the cutting beam's path moved half of `kerf` away from
/// the material (mitcad#1939): an outer boundary out, a hole in, so that
/// the cut leaves the part at its size. Every piece is moved along its
/// normal away from the material (the right of its direction, the
/// material being on the left): a line as a line, an arc as an arc about
/// its centre with its radius grown or shrunk by half the kerf
/// (mitcad#2570), so that a circle stays a circle for the DXF, the SVG
/// and the cutting program's G2 and G3. Polylines, B-splines and an arc
/// that curves into the material no wider than half the kerf are
/// flattened (within their deflection) and their segments moved as lines.
/// Where two lines meet, the corner is mitred, a mitre longer than four
/// times the offset bevelled; where an arc meets another piece at a
/// corner, the path turns round the outside of the corner on an arc of
/// half the kerf about it, and on the inside of a corner the two moved
/// pieces end where they cross. A kerf of 0 leaves the loops as they are.
pub fn kerf_compensated(loops: &[OutlineLoop], kerf: f64) -> Vec<OutlineLoop> {
    if kerf.is_nan() || kerf <= 0.0 {
        return loops.to_vec();
    }
    let d = kerf / 2.0;
    loops
        .iter()
        .map(|l| OutlineLoop {
            outer: l.outer,
            pieces: offset_loop(&l.pieces, d),
        })
        .collect()
}

/// The unit normal right of a direction, None for no direction.
fn right_of(e: P2) -> Option<P2> {
    let len = norm(e);
    (len > 1e-12).then_some([e[1] / len, -e[0] / len])
}

/// The lines between points that lie apart.
fn push_lines(points: &[P2], out: &mut Vec<Piece>) {
    for w in points.windows(2) {
        if dist(w[0], w[1]) > 1e-12 {
            out.push(Piece::Line(w[0], w[1]));
        }
    }
}

/// A loop's pieces as the lines and arcs its offset by `d` moves: lines
/// and arcs as they are, an arc curving into the material no wider than
/// `d` and every other piece as the lines between its points.
fn offset_elements(pieces: &[Piece], d: f64) -> Vec<Piece> {
    let mut out = Vec::with_capacity(pieces.len());
    for piece in pieces {
        match piece {
            Piece::Line(a, b) => push_lines(&[*a, *b], &mut out),
            // A clockwise arc has the material outside its circle: the
            // path runs inside it, half the kerf nearer the centre.
            Piece::Arc { radius, sweep, .. }
                if radius * sweep.abs() > 1e-12 && (*sweep > 0.0 || radius - d > KERF_JOINT) =>
            {
                out.push(piece.clone())
            }
            _ => push_lines(&piece.flattened(), &mut out),
        }
    }
    out
}

/// A line or an arc moved `d` to its right.
fn moved_right(piece: &Piece, d: f64) -> Piece {
    match piece {
        Piece::Line(a, b) => {
            let n = scale(right_of(sub(*b, *a)).unwrap_or([0.0, 0.0]), d);
            Piece::Line(add(*a, n), add(*b, n))
        }
        Piece::Arc {
            center,
            radius,
            start,
            sweep,
        } => Piece::Arc {
            center: *center,
            radius: radius + d * sweep.signum(),
            start: *start,
            sweep: *sweep,
        },
        other => other.clone(),
    }
}

/// The unit direction a line or an arc runs in at its start, or at its
/// end with `at_end`.
fn direction(piece: &Piece, at_end: bool) -> P2 {
    match piece {
        Piece::Arc { start, sweep, .. } => {
            let a = if at_end { start + sweep } else { *start };
            let s = sweep.signum();
            [-a.sin() * s, a.cos() * s]
        }
        _ => {
            let e = sub(piece.end(), piece.start());
            let l = norm(e);
            if l > 0.0 {
                scale(e, 1.0 / l)
            } else {
                [1.0, 0.0]
            }
        }
    }
}

/// Where the line through `p` and `q` crosses the circle about `c` of
/// radius `r`.
fn line_circle(p: P2, q: P2, c: P2, r: f64) -> Vec<P2> {
    let e = sub(q, p);
    let l = norm(e);
    if l <= 1e-12 {
        return Vec::new();
    }
    let u = scale(e, 1.0 / l);
    let foot = add(p, scale(u, dot2(sub(c, p), u)));
    let h2 = r * r - dot2(sub(c, foot), sub(c, foot));
    if h2 < -1e-12 * r.max(1.0) {
        return Vec::new();
    }
    let h = h2.max(0.0).sqrt();
    vec![sub(foot, scale(u, h)), add(foot, scale(u, h))]
}

/// Where two circles cross.
fn circle_circle(c1: P2, r1: f64, c2: P2, r2: f64) -> Vec<P2> {
    let e = sub(c2, c1);
    let l = norm(e);
    if l <= 1e-12 {
        return Vec::new();
    }
    let a = (r1 * r1 - r2 * r2 + l * l) / (2.0 * l);
    let h2 = r1 * r1 - a * a;
    if h2 < -1e-12 * r1.max(1.0) {
        return Vec::new();
    }
    let h = h2.max(0.0).sqrt();
    let m = add(c1, scale(e, a / l));
    let across = [-e[1] / l, e[0] / l];
    vec![add(m, scale(across, h)), sub(m, scale(across, h))]
}

/// Where the line or circle of one moved piece crosses the other's (two
/// lines are mitred, not crossed here).
fn meets(a: &Piece, b: &Piece) -> Vec<P2> {
    match (a, b) {
        (Piece::Line(p, q), Piece::Arc { center, radius, .. })
        | (Piece::Arc { center, radius, .. }, Piece::Line(p, q)) => {
            line_circle(*p, *q, *center, *radius)
        }
        (
            Piece::Arc {
                center: c1,
                radius: r1,
                ..
            },
            Piece::Arc {
                center: c2,
                radius: r2,
                ..
            },
        ) => circle_circle(*c1, *r1, *c2, *r2),
        _ => Vec::new(),
    }
}

/// A moved line or arc cut back to end at `p` (to start there unless
/// `at_end`), a point on its line or circle; None when that would leave
/// nothing of it or lengthen it.
fn trimmed(piece: &Piece, p: P2, at_end: bool) -> Option<Piece> {
    match piece {
        Piece::Line(a, b) => {
            let e = sub(*b, *a);
            let l2 = dot2(e, e);
            let t = if l2 > 0.0 {
                dot2(sub(p, *a), e) / l2
            } else {
                0.0
            };
            let kept = if at_end { t } else { 1.0 - t };
            let shortened = kept > 1e-12 && kept <= 1.0 + 1e-9;
            if !shortened {
                return None;
            }
            Some(if at_end {
                Piece::Line(*a, p)
            } else {
                Piece::Line(p, *b)
            })
        }
        Piece::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            let angle = (p[1] - center[1]).atan2(p[0] - center[0]);
            let wrap = |x: f64| (x + PI).rem_euclid(TAU) - PI;
            let (from, by) = if at_end {
                (*start, sweep + wrap(angle - (start + sweep)))
            } else {
                let delta = wrap(angle - start);
                (start + delta, sweep - delta)
            };
            (by * sweep > 0.0 && by.abs() <= sweep.abs() + 1e-9).then_some(Piece::Arc {
                center: *center,
                radius: *radius,
                start: from,
                sweep: by,
            })
        }
        _ => None,
    }
}

/// A closed loop's pieces moved `d` to their right (see
/// [`kerf_compensated`]).
fn offset_loop(pieces: &[Piece], d: f64) -> Vec<Piece> {
    let elements = offset_elements(pieces, d);
    let n = elements.len();
    if n == 0 {
        return elements;
    }
    let mut moved: Vec<Piece> = elements.iter().map(|p| moved_right(p, d)).collect();
    // What joins the piece before each to it where their ends part.
    let mut joints: Vec<Option<Piece>> = vec![None; n];
    for k in 0..n {
        let prev = (k + n - 1) % n;
        let corner = elements[k].start();
        let (before, after) = (&elements[prev], &elements[k]);
        if n > 1
            && let (Piece::Line(..), Piece::Line(..)) = (before, after)
        {
            let n0 = right_of(direction(before, true)).unwrap_or([0.0, 0.0]);
            let n1 = right_of(direction(after, false)).unwrap_or([0.0, 0.0]);
            let s = 1.0 + dot2(n0, n1);
            let miter = scale(add(n0, n1), 1.0 / s);
            let (end, start) = if s > 1e-9 && norm(miter) <= 4.0 {
                let p = add(corner, scale(miter, d));
                (p, p)
            } else {
                let (p, q) = (add(corner, scale(n0, d)), add(corner, scale(n1, d)));
                joints[k] = Some(Piece::Line(p, q));
                (p, q)
            };
            moved[prev] = Piece::Line(moved[prev].start(), end);
            moved[k] = Piece::Line(start, moved[k].end());
            continue;
        }
        let (end, start) = (moved[prev].end(), moved[k].start());
        if dist(end, start) <= KERF_JOINT {
            continue;
        }
        let turn = cross2(direction(before, true), direction(after, false));
        let (u, v) = (sub(end, corner), sub(start, corner));
        if turn > 1e-9 && cross2(u, v) > 0.0 {
            // The outside of the corner: the beam turns about it.
            joints[k] = Some(Piece::Arc {
                center: corner,
                radius: d,
                start: u[1].atan2(u[0]),
                sweep: cross2(u, v).atan2(dot2(u, v)),
            });
            continue;
        }
        // The inside: both end where they cross nearest the corner.
        let near = lerp(end, start, 0.5);
        let cut = meets(&moved[prev], &moved[k])
            .into_iter()
            .min_by(|p, q| dist(*p, near).total_cmp(&dist(*q, near)))
            .and_then(|p| {
                Some((
                    trimmed(&moved[prev], p, true)?,
                    trimmed(&moved[k], p, false)?,
                ))
            });
        match cut {
            Some((x, y)) if turn < -1e-9 => {
                moved[prev] = x;
                moved[k] = y;
            }
            // They cannot be cut back to one point: a straight line from
            // one to the other, which the cutting checks find too near
            // the part.
            _ => joints[k] = Some(Piece::Line(end, start)),
        }
    }
    let mut out = Vec::with_capacity(2 * n);
    for (joint, piece) in joints.into_iter().zip(moved) {
        out.extend(joint);
        out.push(piece);
    }
    out
}

/// Where parts of these sizes (their boxes' widths and heights at no
/// turn, mm) go on a sheet `width` wide (mitcad#1939, simple automatic
/// nesting): each turned so that its longer side lies along the sheet's
/// width when it fits so, then placed in rows from the sheet's lower left
/// corner, the tallest first, `gap` apart and from the edges. A part wider
/// than the sheet starts a row of its own. Each part's place (its box's
/// middle) and turn (0 or 90 degrees), in the order given.
pub fn nest(sizes: &[(f64, f64)], width: f64, gap: f64) -> Vec<(P2, f64)> {
    let turned: Vec<(f64, f64, f64)> = sizes
        .iter()
        .map(|&(w, h)| {
            if h > w && h + 2.0 * gap <= width {
                (h, w, 90.0)
            } else {
                (w, h, 0.0)
            }
        })
        .collect();
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|a, b| turned[*b].1.total_cmp(&turned[*a].1).then(a.cmp(b)));
    let mut out = vec![([0.0, 0.0], 0.0); sizes.len()];
    let (mut x, mut y, mut row) = (gap, gap, 0.0f64);
    for i in order {
        let (w, h, degrees) = turned[i];
        if x > gap && x + w + gap > width {
            x = gap;
            y += row + gap;
            row = 0.0;
        }
        out[i] = ([x + w / 2.0, y + h / 2.0], degrees);
        x += w + gap;
        row = row.max(h);
    }
    out
}

#[cfg(test)]
mod kerf_tests {
    use std::f64::consts::FRAC_PI_2;

    use super::*;

    fn square(outer: bool, size: f64) -> OutlineLoop {
        let p = [[0.0, 0.0], [size, 0.0], [size, size], [0.0, size]];
        let points: Vec<P2> = if outer {
            p.to_vec()
        } else {
            p.iter().rev().copied().collect()
        };
        OutlineLoop {
            outer,
            pieces: (0..4)
                .map(|i| Piece::Line(points[i], points[(i + 1) % 4]))
                .collect(),
        }
    }

    /// A 10 mm square with a kerf of 0.2: its outer path 10.2 square, a
    /// hole's 9.8 square.
    #[test]
    fn outer_loops_move_out_and_holes_in() {
        let out = kerf_compensated(&[square(true, 10.0), square(false, 10.0)], 0.2);
        assert!((out[0].signed_area() - 10.2 * 10.2).abs() < 1e-9);
        assert!((out[1].signed_area() + 9.8 * 9.8).abs() < 1e-9);
        assert_eq!(
            kerf_compensated(&[square(true, 10.0)], 0.0)[0],
            square(true, 10.0)
        );
    }

    fn arc(center: P2, radius: f64, start: f64, sweep: f64) -> Piece {
        Piece::Arc {
            center,
            radius,
            start,
            sweep,
        }
    }

    /// Every point of a moved loop lies `d` from the part's loop, no
    /// nearer and no farther: the beam's path is the exact offset. Where
    /// two lines meet the path is mitred (see [`kerf_compensated`]): the
    /// points of `mitres`, each at a right-angled corner of the part, lie
    /// `d` times the square root of two from it.
    fn exactly_off(path: &OutlineLoop, part: &OutlineLoop, d: f64, mitres: &[P2]) {
        let points = path.polygon();
        for m in mitres {
            assert!(
                points.iter().any(|p| dist(*p, *m) < 1e-9),
                "no mitre at {m:?}: {path:?}"
            );
        }
        for p in points {
            let nearest = part
                .pieces
                .iter()
                .map(|piece| piece.distance(p))
                .fold(f64::INFINITY, f64::min);
            let wanted = if mitres.iter().any(|m| dist(p, *m) < 1e-9) {
                d * 2f64.sqrt()
            } else {
                d
            };
            assert!((nearest - wanted).abs() < 1e-9, "{p:?} {nearest}: {path:?}");
        }
        // End to end.
        let n = path.pieces.len();
        for i in 0..n {
            let (a, b) = (path.pieces[i].end(), path.pieces[(i + 1) % n].start());
            assert!(dist(a, b) < 1e-6, "{i}: {path:?}");
        }
    }

    fn arcs(l: &OutlineLoop) -> Vec<(P2, f64, f64)> {
        l.pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Arc {
                    center,
                    radius,
                    sweep,
                    ..
                } => Some((*center, *radius, *sweep)),
                _ => None,
            })
            .collect()
    }

    /// Arcs stay arcs (mitcad#2570): a plate with rounded corners grows
    /// its corners' radii by half the kerf, a round hole shrinks by it,
    /// each still one circle about its centre.
    #[test]
    fn arcs_move_as_arcs_about_their_centres() {
        let (r, d) = (3.0, 0.75);
        // A 40 x 20 plate with corners rounded 3, counter-clockwise.
        let mut pieces = Vec::new();
        let corners = [[37.0, 3.0], [37.0, 17.0], [3.0, 17.0], [3.0, 3.0]];
        for (k, c) in corners.iter().enumerate() {
            let a = (k as f64 - 1.0) * FRAC_PI_2;
            let before = polar(*c, r, a);
            let prev = corners[(k + 3) % 4];
            pieces.push(Piece::Line(polar(prev, r, a), before));
            pieces.push(arc(*c, r, a, FRAC_PI_2));
        }
        let plate = OutlineLoop {
            outer: true,
            pieces,
        };
        let hole = OutlineLoop {
            outer: false,
            pieces: vec![arc([20.0, 10.0], 4.0, 0.0, -TAU)],
        };
        let out = kerf_compensated(&[plate.clone(), hole.clone()], 2.0 * d);
        assert_eq!(out[0].pieces.len(), 8, "no joint at a tangent");
        for (center, radius, sweep) in arcs(&out[0]) {
            assert!(corners.contains(&center));
            assert!((radius - (r + d)).abs() < 1e-12);
            assert!((sweep - FRAC_PI_2).abs() < 1e-12);
        }
        assert_eq!(arcs(&out[1]), vec![([20.0, 10.0], 4.0 - d, -TAU)]);
        exactly_off(&out[0], &plate, d, &[]);
        exactly_off(&out[1], &hole, d, &[]);
    }

    /// Where an arc meets a line at a corner (mitcad#2570): round the
    /// outside of the corner the beam turns about it on an arc of half the
    /// kerf; on the inside the moved arc and line end where they cross.
    #[test]
    fn corners_at_arcs_are_turned_round_or_cut_back() {
        let d = 0.5;
        // A 40 x 20 plate with a half-round notch of radius 5 in the
        // middle of its lower edge: the notch's corners are on the
        // outside.
        let plate = OutlineLoop {
            outer: true,
            pieces: vec![
                Piece::Line([0.0, 0.0], [15.0, 0.0]),
                arc([20.0, 0.0], 5.0, PI, -PI),
                Piece::Line([25.0, 0.0], [40.0, 0.0]),
                Piece::Line([40.0, 0.0], [40.0, 20.0]),
                Piece::Line([40.0, 20.0], [0.0, 20.0]),
                Piece::Line([0.0, 20.0], [0.0, 0.0]),
            ],
        };
        let out = &kerf_compensated(std::slice::from_ref(&plate), 2.0 * d)[0];
        // The plate's square corners, where two lines meet, are mitred.
        let mitres = [
            [-d, -d],
            [40.0 + d, -d],
            [40.0 + d, 20.0 + d],
            [-d, 20.0 + d],
        ];
        exactly_off(out, &plate, d, &mitres);
        let turns: Vec<(P2, f64, f64)> = arcs(out);
        assert_eq!(turns.len(), 3, "{out:?}");
        assert!(turns.contains(&([20.0, 0.0], 4.5, -PI)), "{turns:?}");
        for corner in [[15.0, 0.0], [25.0, 0.0]] {
            assert!(
                turns.iter().any(|(c, r, s)| dist(*c, corner) < 1e-12
                    && *r == d
                    && (s - FRAC_PI_2).abs() < 1e-12),
                "{turns:?}"
            );
        }

        // A D-shaped hole: its corners are on the inside.
        let hole = OutlineLoop {
            outer: false,
            pieces: vec![
                arc([0.0, 0.0], 5.0, PI, -PI),
                Piece::Line([5.0, 0.0], [-5.0, 0.0]),
            ],
        };
        let out = &kerf_compensated(std::slice::from_ref(&hole), 2.0 * d)[0];
        exactly_off(out, &hole, d, &[]);
        assert_eq!(out.pieces.len(), 2, "{out:?}");
        let x = 20f64.sqrt();
        assert!(dist(out.pieces[0].start(), [-x, 0.5]) < 1e-12, "{out:?}");
        assert!(dist(out.pieces[0].end(), [x, 0.5]) < 1e-12, "{out:?}");
        assert!(matches!(out.pieces[0], Piece::Arc { radius, .. } if radius == 4.5));
    }

    /// An arc curving into the material tighter than half the kerf cannot
    /// be followed: it is moved as the lines of its points, as before
    /// (mitcad#2570), and the path comes nearer the part than half the
    /// kerf, which the cutting checks refuse.
    #[test]
    fn a_hole_narrower_than_the_kerf_is_not_kept_as_an_arc() {
        let hole = OutlineLoop {
            outer: false,
            pieces: vec![arc([0.0, 0.0], 1.0, 0.0, -TAU)],
        };
        let out = &kerf_compensated(&[hole], 3.0)[0];
        assert!(arcs(out).is_empty());
        assert!(out.pieces.iter().all(|p| matches!(p, Piece::Line(..))));
    }

    /// Marks laid on a part (mitcad#1939): a sketch's polyline taken into
    /// the outline's coordinates (mirrored for a back view), moved with the
    /// part, and written on its profile's engrave and score layers; a
    /// Flat Pattern part's bend lines on the profile's crease layers.
    #[test]
    fn marks_and_bends_take_the_profiles_layers() {
        let frame = MarkFrame {
            transform: None,
            origin: [0.0, 0.0, 5.0],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            mirror: false,
        };
        assert_eq!(frame.point([3.0, 4.0, 5.0]), [3.0, 4.0]);
        let back = MarkFrame {
            mirror: true,
            ..frame.clone()
        };
        assert_eq!(back.point([3.0, 4.0, 5.0]), [-3.0, 4.0]);
        let marks = frame.marks(
            MarkKind::Engrave,
            &[
                vec![[1.0, 1.0, 5.0], [2.0, 1.0, 5.0]],
                vec![[0.0, 0.0, 5.0]],
            ],
        );
        assert_eq!(marks.len(), 1, "a single point is no mark");
        let view = PartView {
            outline: Outline {
                loops: vec![square(true, 10.0)],
            },
            ..PartView::default()
        };
        // The middle (5, 5) placed at (100, 100), not turned.
        let placed = placed_marks(&view, &marks, [100.0, 100.0], 0.0);
        assert_eq!(placed[0].piece, Piece::Line([96.0, 96.0], [97.0, 96.0]));
        let part = PlacedPart {
            id: 1,
            name: "Plate".into(),
            loops: vec![square(true, 10.0)],
            bends: vec![BendLine {
                start: [0.0, 5.0],
                end: [10.0, 5.0],
                up: false,
            }],
            marks: vec![
                placed[0].clone(),
                Mark {
                    kind: MarkKind::Score,
                    piece: Piece::Line([1.0, 2.0], [8.0, 2.0]),
                },
            ],
            process: SheetProcess::LaserSheet,
        };
        let drawing = dxf(std::slice::from_ref(&part), false);
        let on = |layer: &str| drawing.entities.iter().filter(|e| e.layer == layer).count();
        assert_eq!(on("ENGRAVE"), 1);
        assert_eq!(on("SCORE"), 2, "the bend line and the scored mark");
        let board = PlacedPart {
            process: SheetProcess::Board,
            marks: Vec::new(),
            ..part
        };
        let drawing = dxf(&[board], false);
        assert_eq!(
            drawing
                .entities
                .iter()
                .filter(|e| e.layer == "CREASE_REVERSE")
                .count(),
            1
        );
    }

    #[test]
    fn parts_are_nested_in_rows_the_tallest_first() {
        // A 1000 wide sheet: 300 x 200, a tall 100 x 400 turned, 500 x 50.
        let places = nest(
            &[(300.0, 200.0), (100.0, 400.0), (500.0, 50.0)],
            1000.0,
            5.0,
        );
        assert_eq!(places[1].1, 90.0);
        // The tallest first: 300 x 200 at the corner, the turned 400 x 100
        // beside it, the 500 x 50 on a row of its own above.
        assert_eq!(places[0], ([155.0, 105.0], 0.0));
        assert_eq!(places[1], ([510.0, 55.0], 90.0));
        assert_eq!(places[2], ([255.0, 235.0], 0.0));
    }
}
