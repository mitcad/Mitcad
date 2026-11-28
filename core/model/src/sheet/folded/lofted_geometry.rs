// SPDX-License-Identifier: MIT
//! The geometry of a Lofted Flange (mitcad#2512), apart from the model:
//! the profiles as chains of curves in the body's coordinates, the
//! transition between them as regions (strips) from the first profile to
//! the second, its seam and gap, its facets, its surfaces at a depth in
//! the material, their development and the patches of the folded body.
//!
//! - **Strips.** A strip runs between a piece of the first profile and a
//!   piece of the second (its sides), each a point, a straight segment or
//!   an arc; its lines from one side to the other at the same fraction `s`
//!   of both sides are straight lines of the surface (rulings). A strip of
//!   points and segments is flat (a triangle or a trapezoid), a point
//!   against an arc a cone, two arcs a cone or a cylinder between circles
//!   in parallel planes. Every strip develops without stretching.
//! - **Matched transition.** Profiles in parallel planes that turn one way
//!   (convex: lines and arcs turning left, corners turning left) are joined
//!   where they run in the same direction: a line against a line is a
//!   trapezoid, a line against a corner or a point of an arc a triangle, a
//!   corner against an arc a cone, an arc against an arc a cone or a
//!   cylinder. This surface is developable and smooth except where corners
//!   meet corners; die formed, it is the part. Press brake, each cone and
//!   arc strip is cut into flat facets (triangles from the corner, or
//!   trapezoids between the arcs' chords, which are parallel).
//! - **Triangulated transition** (press brake only): any other profiles,
//!   on angled planes or not convex, are cut into polylines (arcs and
//!   other curves into chords) and joined by triangles, each time with the
//!   shorter diagonal.
//! - **Seam and gap.** A closed transition is cut open along a ruling (the
//!   seam: the shortest, or the one through the point at a fraction of the
//!   first profile's length), and both profiles lose `gap / 2` on each
//!   side of it. Where the two cut points lie in different strips, the
//!   part between them is filled by triangles and cones from the farther
//!   cut point (still developable; it meets its neighbours at slight folds).
//! - **Depth.** The surface `h` behind the front along the material
//!   direction is the strips' surface moved along their normals; at a fold
//!   between strips both meet on the mitre (the normal's sum over one plus
//!   their dot product), so a flat strip moved stays flat and exact. A
//!   curved strip meeting a fold keeps a flat sliver between its moved
//!   ruling and the mitre.
//! - **Development.** Flat bands are laid down exactly; a curved band is
//!   laid down as a strip of thin triangles, at two step counts, the
//!   results extrapolated (Richardson) to well below a micrometre.
//! - **Bends** (mitcad#2560, [`bent`]): the press brake's folds made
//!   cylinders of the rules' radius tangent to the facets beside them, the
//!   facets cut back to them, each bend's sides where its cylinder meets
//!   the profiles' planes ([`Section`]); a bend whose inside is on the back
//!   develops deeper ([`Transition::shift`]) so it lies flat at the
//!   K-factor from its inside.
//! - **Ruled transition** (mitcad#2560, [`ruled_transition`]): for a die
//!   formed transition laid flat by triangulation, the lines between the
//!   points at the same share of both profiles' lengths, in bands between
//!   corners; made solid by itself ([`ruled_body`], [`RuledBack`]): each
//!   band's offset, two bands' offsets at a crease stretched or cut back to
//!   the line where they meet (found in the planes square to the crease),
//!   the walls from the front's sides to the back's.

use std::collections::VecDeque;
use std::f64::consts::{PI, TAU};

use mitcad_vec3::{add, cross, distance, dot, norm, scale, sub};

use crate::ids::FeatureUid;
use crate::kernel::SurfacePatch;
use crate::profile::{SegmentGeometry, SketchFrame};
use crate::sheet::BoundedCurve2Ext;
use crate::sketch::geometry::{Curve2, angle_after};
use crate::topo::{FaceName, RoleKey};

pub(super) type V3 = [f64; 3];
pub(super) type P2 = [f64; 2];

/// Points closer than this are one (mm).
pub(super) const NEAR: f64 = 1e-6;
/// Angles below this are none (radians).
const FLAT: f64 = 1e-9;
/// Folds sharper than this are refused (radians).
const SHARPEST: f64 = 170.0 * PI / 180.0;
/// Steps of a curved band's development (and twice as many).
const STEPS: usize = 1024;
/// Spans of a curved band's curves (in the chart and in the body): cubic
/// spans through exact points with exact tangents, within about 1e-6 mm
/// of a quarter circle of 100 mm radius in 32 spans.
const SPANS: usize = 128;

fn lerp3(a: V3, b: V3, s: f64) -> V3 {
    add(a, scale(sub(b, a), s))
}

fn unit3(v: V3) -> Option<V3> {
    let n = norm(v);
    (n > 1e-300 && n.is_finite()).then(|| scale(v, 1.0 / n))
}

/// An arc of a circle: `center + radius (cos θ x + sin θ y)` for θ from
/// `from` to `to` (either way round).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ArcPiece {
    pub center: V3,
    pub x: V3,
    pub y: V3,
    pub radius: f64,
    pub from: f64,
    pub to: f64,
}

impl ArcPiece {
    fn at(&self, theta: f64) -> V3 {
        let (s, c) = theta.sin_cos();
        add(
            self.center,
            add(
                scale(self.x, self.radius * c),
                scale(self.y, self.radius * s),
            ),
        )
    }

    pub fn point(&self, s: f64) -> V3 {
        self.at(self.from + (self.to - self.from) * s)
    }

    /// The derivative by the fraction `s`.
    pub fn derivative(&self, s: f64) -> V3 {
        let theta = self.from + (self.to - self.from) * s;
        let (sn, c) = theta.sin_cos();
        scale(
            add(scale(self.x, -sn), scale(self.y, c)),
            self.radius * (self.to - self.from),
        )
    }

    pub fn length(&self) -> f64 {
        self.radius * (self.to - self.from).abs()
    }

    fn part(&self, s0: f64, s1: f64) -> Self {
        let d = self.to - self.from;
        Self {
            from: self.from + d * s0,
            to: self.from + d * s1,
            ..*self
        }
    }

    fn reversed(&self) -> Self {
        Self {
            from: self.to,
            to: self.from,
            ..*self
        }
    }

    /// The chords of `n` facets.
    fn chords(&self, n: usize) -> Vec<(V3, V3)> {
        (0..n)
            .map(|i| {
                (
                    self.point(i as f64 / n as f64),
                    self.point((i + 1) as f64 / n as f64),
                )
            })
            .collect()
    }
}

/// A curve of a profile in the body's coordinates.
#[derive(Debug, Clone)]
pub(super) enum Piece {
    Line(V3, V3),
    Arc(ArcPiece),
    /// Another sketch curve (an ellipse, a spline) placed by `frame`, from
    /// its parameter `from` to `to`.
    Other {
        curve: Curve2,
        frame: SketchFrame,
        from: f64,
        to: f64,
    },
}

fn frame_vector(frame: &SketchFrame, v: P2) -> V3 {
    add(scale(frame.x_axis, v[0]), scale(frame.y_axis, v[1]))
}

impl Piece {
    pub fn point(&self, s: f64) -> V3 {
        match self {
            Self::Line(a, b) => lerp3(*a, *b, s),
            Self::Arc(a) => a.point(s),
            Self::Other {
                curve,
                frame,
                from,
                to,
            } => frame.point(curve.point(from + (to - from) * s)),
        }
    }

    /// The derivative by the fraction `s`.
    pub fn derivative(&self, s: f64) -> V3 {
        match self {
            Self::Line(a, b) => sub(*b, *a),
            Self::Arc(a) => a.derivative(s),
            Self::Other {
                curve,
                frame,
                from,
                to,
            } => {
                let d = curve.eval(from + (to - from) * s)[1];
                scale(frame_vector(frame, d), to - from)
            }
        }
    }

    pub fn start(&self) -> V3 {
        self.point(0.0)
    }

    pub fn end(&self) -> V3 {
        self.point(1.0)
    }

    pub fn reversed(&self) -> Self {
        match self {
            Self::Line(a, b) => Self::Line(*b, *a),
            Self::Arc(a) => Self::Arc(a.reversed()),
            Self::Other {
                curve,
                frame,
                from,
                to,
            } => Self::Other {
                curve: curve.clone(),
                frame: *frame,
                from: *to,
                to: *from,
            },
        }
    }

    pub fn length(&self) -> f64 {
        match self {
            Self::Line(a, b) => distance(*a, *b),
            Self::Arc(a) => a.length(),
            Self::Other { .. } => {
                let n = 256;
                (0..n)
                    .map(|i| {
                        distance(
                            self.point(i as f64 / n as f64),
                            self.point((i + 1) as f64 / n as f64),
                        )
                    })
                    .sum()
            }
        }
    }

    /// Points along it, its ends included.
    pub fn samples(&self, n: usize) -> Vec<V3> {
        (0..=n).map(|i| self.point(i as f64 / n as f64)).collect()
    }
}

/// A sketch curve as a piece of a profile.
pub(super) fn piece_of_curve(curve: &Curve2, frame: &SketchFrame) -> Piece {
    let arc = |center: P2, radius: f64, from: f64, to: f64| {
        Piece::Arc(ArcPiece {
            center: frame.point(center),
            x: frame.x_axis,
            y: frame.y_axis,
            radius,
            from,
            to,
        })
    };
    match curve {
        Curve2::Line { a, b } => Piece::Line(frame.point(*a), frame.point(*b)),
        Curve2::Circle {
            center,
            radius,
            start,
        } => arc(*center, *radius, *start, start + TAU),
        Curve2::Arc {
            center,
            radius,
            start,
            end,
        } => arc(*center, *radius, *start, angle_after(*start, *end)),
        other => {
            let (from, to) = other.domain();
            Piece::Other {
                curve: other.clone(),
                frame: *frame,
                from,
                to,
            }
        }
    }
}

/// A region's boundary curve as a piece of a profile.
pub(super) fn piece_of_segment(
    geometry: &SegmentGeometry,
    frame: &SketchFrame,
) -> Result<Piece, String> {
    let arc = |center: P2, radius: f64, from: f64, to: f64| {
        Piece::Arc(ArcPiece {
            center: frame.point(center),
            x: frame.x_axis,
            y: frame.y_axis,
            radius,
            from,
            to,
        })
    };
    Ok(match geometry {
        SegmentGeometry::Line { start, end } => Piece::Line(frame.point(*start), frame.point(*end)),
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => arc(
            *center,
            *radius,
            *start_angle,
            angle_after(*start_angle, *end_angle),
        ),
        SegmentGeometry::Circle {
            center,
            radius,
            start,
        } => arc(*center, *radius, *start, start + TAU),
        other => {
            let bounded = crate::sheet::BoundedCurve2::from_geometry(other.clone())?;
            Piece::Other {
                curve: bounded.support()?,
                frame: *frame,
                from: bounded.range.first,
                to: bounded.range.last,
            }
        }
    })
}

/// A profile: its pieces joined end to end, in order, and whether it
/// closes.
#[derive(Debug, Clone)]
pub(super) struct Chain {
    pub pieces: Vec<Piece>,
    pub closed: bool,
    /// The normal of the sketch plane it lies in.
    pub normal: V3,
}

impl Chain {
    pub fn length(&self) -> f64 {
        self.pieces.iter().map(Piece::length).sum()
    }

    pub fn reversed(&self) -> Self {
        Self {
            pieces: self.pieces.iter().rev().map(Piece::reversed).collect(),
            closed: self.closed,
            normal: self.normal,
        }
    }

    /// Points along it.
    pub fn samples(&self) -> Vec<V3> {
        let mut out = Vec::new();
        for p in &self.pieces {
            let n = if matches!(p, Piece::Line(..)) { 1 } else { 64 };
            let s = p.samples(n);
            out.extend_from_slice(&s[..n]);
        }
        if !self.closed
            && let Some(last) = self.pieces.last()
        {
            out.push(last.end());
        }
        out
    }

    /// The area vector of a closed chain (twice its area along its normal).
    fn area_vector(&self) -> V3 {
        let points = self.samples();
        let mut sum = [0.0; 3];
        for (i, p) in points.iter().enumerate() {
            sum = add(sum, cross(*p, points[(i + 1) % points.len()]));
        }
        sum
    }
}

fn near(a: V3, b: V3) -> bool {
    distance(a, b) <= NEAR
}

/// The pieces joined end to end, in the order they run from the first
/// piece's start (a first piece in the middle of an open chain is turned
/// to run on). `what` names the profile in errors.
pub(super) fn chain(mut pieces: Vec<Piece>, normal: V3, what: &str) -> Result<Chain, String> {
    if pieces.is_empty() {
        return Err(format!("the {what} profile has no curves"));
    }
    if let Some(short) = pieces.iter().position(|p| p.length() <= NEAR) {
        return Err(format!(
            "curve {} of the {what} profile has no length",
            short + 1
        ));
    }
    let first = pieces.remove(0);
    let mut out = VecDeque::from([first]);
    loop {
        let end = out.back().expect("a piece").end();
        let start = out.front().expect("a piece").start();
        if near(start, end) && out.len() > 1 {
            break;
        }
        if let Some(i) = pieces
            .iter()
            .position(|p| near(p.start(), end) || near(p.end(), end))
        {
            let p = pieces.remove(i);
            out.push_back(if near(p.start(), end) {
                p
            } else {
                p.reversed()
            });
            continue;
        }
        if let Some(i) = pieces
            .iter()
            .position(|p| near(p.end(), start) || near(p.start(), start))
        {
            let p = pieces.remove(i);
            out.push_front(if near(p.end(), start) {
                p
            } else {
                p.reversed()
            });
            continue;
        }
        break;
    }
    if !pieces.is_empty() {
        return Err(format!(
            "the {what} profile's curves are not joined end to end in one chain"
        ));
    }
    let pieces: Vec<Piece> = out.into_iter().collect();
    let closed = near(pieces[0].start(), pieces[pieces.len() - 1].end());
    Ok(Chain {
        pieces,
        closed,
        normal,
    })
}

/// The second profile turned to run the way the first does: round the
/// same way (closed), or from the end nearer the first's start (open).
pub(super) fn aligned(first: &Chain, second: Chain) -> Result<Chain, String> {
    if first.closed != second.closed {
        return Err(
            "the profiles must be both closed or both open: a lofted flange joins a closed \
             profile to a closed one, an open one to an open one"
                .into(),
        );
    }
    if first.closed {
        if dot(first.area_vector(), second.area_vector()) < 0.0 {
            return Ok(second.reversed());
        }
        return Ok(second);
    }
    let (a0, a1) = (
        first.pieces[0].start(),
        first.pieces[first.pieces.len() - 1].end(),
    );
    let (b0, b1) = (
        second.pieces[0].start(),
        second.pieces[second.pieces.len() - 1].end(),
    );
    if distance(a0, b0) + distance(a1, b1) > distance(a0, b1) + distance(a1, b0) {
        return Ok(second.reversed());
    }
    Ok(second)
}

/// A plane: a point of it and its unit normal.
pub(super) type Plane = (V3, V3);

/// Where a bend's cylinder meets a profile's plane (mitcad#2560): the
/// cylinder's lines `centre + radius (cos φ p + sin φ q) + λ axis` for φ
/// from `from` to `to`, each met by `plane`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Section {
    pub centre: V3,
    /// The unit direction of the cylinder's lines (the fold's, from the
    /// first profile to the second).
    pub axis: V3,
    /// Unit radial directions at right angles: φ 0 along `p`, φ a quarter
    /// turn along `q`.
    pub p: V3,
    pub q: V3,
    pub radius: f64,
    pub from: f64,
    pub to: f64,
    pub plane: Plane,
}

impl Section {
    fn radial(&self, phi: f64) -> V3 {
        let (s, c) = phi.sin_cos();
        add(scale(self.p, c), scale(self.q, s))
    }

    /// The cylinder's line at φ: its point at `λ = 0`.
    pub fn line(&self, phi: f64) -> V3 {
        add(self.centre, scale(self.radial(phi), self.radius))
    }

    pub fn point(&self, s: f64) -> V3 {
        let x = self.line(self.from + (self.to - self.from) * s);
        let (o, n) = self.plane;
        add(x, scale(self.axis, dot(sub(o, x), n) / dot(self.axis, n)))
    }

    /// The derivative by the fraction `s`.
    pub fn derivative(&self, s: f64) -> V3 {
        let phi = self.from + (self.to - self.from) * s;
        let (sn, c) = phi.sin_cos();
        let round = scale(add(scale(self.p, -sn), scale(self.q, c)), self.radius);
        let n = self.plane.1;
        let along = -dot(round, n) / dot(self.axis, n);
        scale(add(round, scale(self.axis, along)), self.to - self.from)
    }

    pub fn length(&self) -> f64 {
        let n = 64;
        (0..n)
            .map(|i| {
                distance(
                    self.point(i as f64 / n as f64),
                    self.point((i + 1) as f64 / n as f64),
                )
            })
            .sum()
    }

    fn part(&self, s0: f64, s1: f64) -> Self {
        let d = self.to - self.from;
        Self {
            from: self.from + d * s0,
            to: self.from + d * s1,
            ..*self
        }
    }

    fn reversed(&self) -> Self {
        Self {
            from: self.to,
            to: self.from,
            ..*self
        }
    }
}

/// A side of a strip: a piece of one profile, or where a bend of the
/// press brake's folds meets the profile's plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Side {
    Point(V3),
    Segment(V3, V3),
    Arc(ArcPiece),
    Section(Section),
}

impl Side {
    pub fn point(&self, s: f64) -> V3 {
        match self {
            Self::Point(p) => *p,
            Self::Segment(a, b) => lerp3(*a, *b, s),
            Self::Arc(a) => a.point(s),
            Self::Section(c) => c.point(s),
        }
    }

    pub fn derivative(&self, s: f64) -> V3 {
        match self {
            Self::Point(_) => [0.0; 3],
            Self::Segment(a, b) => sub(*b, *a),
            Self::Arc(a) => a.derivative(s),
            Self::Section(c) => c.derivative(s),
        }
    }

    pub fn length(&self) -> f64 {
        match self {
            Self::Point(_) => 0.0,
            Self::Segment(a, b) => distance(*a, *b),
            Self::Arc(a) => a.length(),
            Self::Section(c) => c.length(),
        }
    }

    fn part(&self, s0: f64, s1: f64) -> Self {
        match self {
            Self::Point(p) => Self::Point(*p),
            Self::Segment(a, b) => Self::Segment(lerp3(*a, *b, s0), lerp3(*a, *b, s1)),
            Self::Arc(a) => Self::Arc(a.part(s0, s1)),
            Self::Section(c) => Self::Section(c.part(s0, s1)),
        }
    }

    fn reversed(&self) -> Self {
        match self {
            Self::Point(p) => Self::Point(*p),
            Self::Segment(a, b) => Self::Segment(*b, *a),
            Self::Arc(a) => Self::Arc(a.reversed()),
            Self::Section(c) => Self::Section(c.reversed()),
        }
    }

    fn is_point(&self) -> bool {
        self.length() <= NEAR
    }
}

/// A region of the transition between a piece of the first profile (`a`)
/// and one of the second (`b`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Strip {
    pub a: Side,
    pub b: Side,
}

impl Strip {
    /// Flat: points and segments only.
    pub fn flat(&self) -> bool {
        matches!(self.a, Side::Point(_) | Side::Segment(..))
            && matches!(self.b, Side::Point(_) | Side::Segment(..))
    }

    /// A bend of the press brake's folds (mitcad#2560).
    pub fn bend(&self) -> bool {
        matches!(self.a, Side::Section(_))
    }

    /// Two arcs, whose rulings are only those at the same fraction.
    fn matched(&self) -> bool {
        matches!((self.a, self.b), (Side::Arc(_), Side::Arc(_)))
    }

    pub fn ruling(&self, s: f64) -> (V3, V3) {
        (self.a.point(s), self.b.point(s))
    }

    fn part(&self, s0: f64, s1: f64) -> Self {
        Self {
            a: self.a.part(s0, s1),
            b: self.b.part(s0, s1),
        }
    }

    fn reversed(&self) -> Self {
        Self {
            a: self.a.reversed(),
            b: self.b.reversed(),
        }
    }

    /// The direction it runs along the profiles at `s`.
    fn along(&self, s: f64) -> V3 {
        if self.a.is_point() {
            self.b.derivative(s)
        } else {
            self.a.derivative(s)
        }
    }

    /// The unit normal at `s` (the ruling across the way it runs; at the
    /// middle for a flat strip), or None where the profiles meet.
    pub fn normal(&self, s: f64) -> Option<V3> {
        let s = if self.flat() { 0.5 } else { s };
        let (a, b) = self.ruling(s);
        unit3(cross(sub(b, a), self.along(s)))
    }

    /// Its points, for a place to paint.
    pub fn points(&self) -> Vec<V3> {
        let mut out = Vec::new();
        for i in 0..=8 {
            let (a, b) = self.ruling(i as f64 / 8.0);
            out.push(a);
            out.push(b);
        }
        out
    }
}

/// Why profiles cannot be matched direction by direction; the press brake
/// triangulates them instead.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Unmatched(pub String);

/// An element of a profile by the direction it runs (`psi`, an angle about
/// the common normal, growing as the profile turns left).
#[derive(Debug, Clone, Copy)]
enum Element {
    Line {
        a: V3,
        b: V3,
        psi: f64,
    },
    /// A corner (`point`) or an arc turning from `lo` to `hi`.
    Turn {
        lo: f64,
        hi: f64,
        point: V3,
        arc: Option<ArcPiece>,
    },
}

impl Element {
    fn lo(&self) -> f64 {
        match self {
            Self::Line { psi, .. } => *psi,
            Self::Turn { lo, .. } => *lo,
        }
    }

    fn hi(&self) -> f64 {
        match self {
            Self::Line { psi, .. } => *psi,
            Self::Turn { hi, .. } => *hi,
        }
    }

    fn shifted(&self, by: f64) -> Self {
        match *self {
            Self::Line { a, b, psi } => Self::Line {
                a,
                b,
                psi: psi + by,
            },
            Self::Turn { lo, hi, point, arc } => Self::Turn {
                lo: lo + by,
                hi: hi + by,
                point,
                arc,
            },
        }
    }

    /// The part of a turn from direction `from` to `to`.
    fn turn_part(&self, from: f64, to: f64) -> Self {
        match *self {
            Self::Turn { lo, hi, point, arc } => {
                let span = (hi - lo).max(f64::MIN_POSITIVE);
                let arc = arc.map(|a| a.part((from - lo) / span, (to - lo) / span));
                Self::Turn {
                    lo: from,
                    hi: to,
                    point: arc.map_or(point, |a| a.point(0.0)),
                    arc,
                }
            }
            line => line,
        }
    }

    /// The side over a turn's whole span.
    fn side(&self) -> Side {
        match *self {
            Self::Line { a, b, .. } => Side::Segment(a, b),
            Self::Turn { point, arc, .. } => arc.map_or(Side::Point(point), Side::Arc),
        }
    }

    /// The point of a turn at direction `psi`.
    fn point_at(&self, psi: f64) -> V3 {
        match *self {
            Self::Line { a, .. } => a,
            Self::Turn { lo, hi, point, arc } => match arc {
                Some(a) => a.point(((psi - lo) / (hi - lo).max(f64::MIN_POSITIVE)).clamp(0.0, 1.0)),
                None => point,
            },
        }
    }
}

/// The in-plane frame of directions: `e1`, `e2 = n × e1`.
#[derive(Debug, Clone, Copy)]
struct Directions {
    n: V3,
    e1: V3,
    e2: V3,
}

impl Directions {
    fn angle(&self, v: V3) -> f64 {
        dot(v, self.e2).atan2(dot(v, self.e1))
    }
}

fn wrap(angle: f64) -> f64 {
    let mut a = angle.rem_euclid(TAU);
    if a > PI {
        a -= TAU;
    }
    a
}

/// A profile's elements by direction, from `psi0` at its start, or why it
/// does not turn one way.
fn elements(chain: &Chain, dirs: &Directions, what: &str) -> Result<Vec<Element>, Unmatched> {
    let not_convex = || {
        Unmatched(format!(
            "the {what} profile does not turn one way (it is not convex): only such profiles in \
             parallel planes are joined by a smooth developable surface"
        ))
    };
    let mut out: Vec<Element> = Vec::new();
    let mut psi: Option<f64> = None;
    let corner = |out: &mut Vec<Element>, at: V3, from: f64, to: f64| -> Result<(), Unmatched> {
        let turn = to - from;
        if turn < -1e-7 {
            return Err(not_convex());
        }
        if turn > 1e-7 {
            out.push(Element::Turn {
                lo: from,
                hi: to,
                point: at,
                arc: None,
            });
        }
        Ok(())
    };
    let count = chain.pieces.len();
    for (i, piece) in chain.pieces.iter().enumerate() {
        let start_dir = dirs.angle(piece.derivative(0.0));
        // The direction continuous with the last one.
        let start = match psi {
            None => start_dir,
            Some(prev) => prev + wrap(start_dir - prev),
        };
        if let Some(prev) = psi {
            if (start - prev).abs() >= PI - 1e-7 {
                return Err(not_convex());
            }
            corner(&mut out, piece.start(), prev, start)?;
        }
        match piece {
            Piece::Line(a, b) => {
                out.push(Element::Line {
                    a: *a,
                    b: *b,
                    psi: start,
                });
                psi = Some(start);
            }
            Piece::Arc(arc) => {
                // Turning left about n: (x × y)·n with the way it runs.
                let left = dot(cross(arc.x, arc.y), dirs.n) * (arc.to - arc.from);
                if left <= 0.0 {
                    return Err(not_convex());
                }
                let end = start + (arc.to - arc.from).abs();
                out.push(Element::Turn {
                    lo: start,
                    hi: end,
                    point: arc.point(0.0),
                    arc: Some(*arc),
                });
                psi = Some(end);
            }
            Piece::Other { .. } => {
                return Err(Unmatched(format!(
                    "the {what} profile has curves other than lines and arcs: a smooth \
                     developable surface is made between lines and arcs"
                )));
            }
        }
        if chain.closed && i + 1 == count {
            let first = out[0].lo();
            let last = psi.expect("a direction");
            let back = last + wrap(first - last);
            corner(&mut out, chain.pieces[0].start(), last, back)?;
            if ((back - first) - TAU).abs() > 1e-6 {
                return Err(not_convex());
            }
        }
    }
    Ok(out)
}

/// A closed profile's elements shifted and turned round to cover the
/// directions from `start` to `start + 2π`, split where they cross them.
fn rotated_to(elements: &[Element], start: f64) -> Vec<Element> {
    let first = elements[0].lo();
    let shift = ((start - first) / TAU).floor() * TAU;
    let mut all: Vec<Element> = Vec::new();
    for turn in [-1.0, 0.0, 1.0, 2.0] {
        all.extend(elements.iter().map(|e| e.shifted(shift + turn * TAU)));
    }
    let end = start + TAU;
    let tol = 1e-9;
    let mut out = Vec::new();
    for e in all {
        match e {
            Element::Line { psi, .. } => {
                if psi >= start - tol && psi < end - tol {
                    out.push(e);
                }
            }
            Element::Turn { lo, hi, .. } => {
                let (from, to) = (lo.max(start), hi.min(end));
                if to - from > tol {
                    out.push(e.turn_part(from, to));
                }
            }
        }
    }
    out.sort_by(|a, b| {
        a.lo()
            .partial_cmp(&b.lo())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.hi()
                    .partial_cmp(&b.hi())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    out
}

/// The strips of profiles joined where they run the same way, or why they
/// cannot be.
pub(super) fn matched(first: &Chain, second: &Chain) -> Result<Vec<Strip>, Unmatched> {
    let n = first.normal;
    if norm(cross(n, second.normal)) > 1e-9 {
        return Err(Unmatched(
            "the profiles lie on planes that are not parallel: a smooth developable surface is \
             made between profiles in parallel planes"
                .into(),
        ));
    }
    // Turning left about n.
    let mut n = n;
    if first.closed {
        if dot(first.area_vector(), n) < 0.0 {
            n = scale(n, -1.0);
        }
    } else {
        let mut turn = 0.0;
        for w in first.pieces.windows(2) {
            turn += dot(cross(w[0].derivative(1.0), w[1].derivative(0.0)), n);
        }
        for p in &first.pieces {
            if let Piece::Arc(a) = p {
                turn += dot(cross(a.x, a.y), n) * (a.to - a.from);
            }
        }
        if turn < 0.0 {
            n = scale(n, -1.0);
        }
    }
    let t0 = first.pieces[0].derivative(0.0);
    let e1 = unit3(sub(t0, scale(n, dot(t0, n))))
        .ok_or_else(|| Unmatched("the first profile does not lie in its plane".into()))?;
    let dirs = Directions {
        n,
        e1,
        e2: cross(n, e1),
    };
    let a = elements(first, &dirs, "first")?;
    let mut b = elements(second, &dirs, "second")?;
    let start = a[0].lo();
    if first.closed {
        b = rotated_to(&b, start);
    } else {
        // The same directions from start to end.
        let shift = ((start - b[0].lo()) / TAU).round() * TAU;
        b = b.iter().map(|e| e.shifted(shift)).collect();
        let (a_end, b_end) = (a[a.len() - 1].hi(), b[b.len() - 1].hi());
        if (b[0].lo() - start).abs() > 1e-6 || (a_end - b_end).abs() > 1e-6 {
            return Err(Unmatched(
                "the open profiles do not turn through the same directions: a smooth \
                 developable surface joins profiles that run the same way at both ends"
                    .into(),
            ));
        }
    }
    let tol = 1e-9;
    let mut strips = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut psi = start;
    while i < a.len() || j < b.len() {
        let line_at = |e: Option<&Element>| match e {
            Some(Element::Line { psi: at, .. }) => (*at - psi).abs() <= 1e-7,
            _ => false,
        };
        let (ea, eb) = (a.get(i), b.get(j));
        let (a_line, b_line) = (line_at(ea), line_at(eb));
        if a_line || b_line {
            let side = |e: Option<&Element>, line: bool, other: &[Element], k: usize| -> Side {
                match (e, line) {
                    (Some(e), true) => e.side(),
                    (Some(e), false) => Side::Point(e.point_at(psi)),
                    // Past the last element: the profile's end.
                    (None, _) => Side::Point(other.last().map_or([0.0; 3], |l| end_point(l, k))),
                }
            };
            strips.push(Strip {
                a: side(ea, a_line, &a, i),
                b: side(eb, b_line, &b, j),
            });
            if a_line {
                i += 1;
            }
            if b_line {
                j += 1;
            }
            continue;
        }
        let (Some(ea), Some(eb)) = (ea, eb) else {
            return Err(Unmatched(
                "the profiles' directions do not match up at their ends".into(),
            ));
        };
        if ea.lo() > psi + 1e-7 || eb.lo() > psi + 1e-7 {
            return Err(Unmatched(
                "the profiles' directions leave a gap between them".into(),
            ));
        }
        let hi = ea.hi().min(eb.hi());
        if hi - psi > tol {
            let a_side = ea.turn_part(psi, hi).side();
            let b_side = eb.turn_part(psi, hi).side();
            if !(a_side.is_point() && b_side.is_point()) {
                strips.push(Strip {
                    a: a_side,
                    b: b_side,
                });
            }
        }
        psi = hi.max(psi);
        if ea.hi() <= psi + 1e-7 {
            i += 1;
        }
        if eb.hi() <= psi + 1e-7 {
            j += 1;
        }
    }
    if strips.is_empty() {
        return Err(Unmatched("the profiles make no surface".into()));
    }
    Ok(merged(strips, first.closed))
}

/// How far arc `y`'s angles lie from running on from arc `x` on the same
/// circle (whole turns), or None when it does not.
fn runs_on(x: &ArcPiece, y: &ArcPiece) -> Option<f64> {
    let shift = x.to - y.from;
    let turns = (shift / TAU).round() * TAU;
    (distance(x.center, y.center) <= 1e-9
        && (x.radius - y.radius).abs() <= 1e-9
        && distance(x.x, y.x) <= 1e-12
        && distance(x.y, y.y) <= 1e-12
        && (shift - turns).abs() <= 1e-9
        && (y.to - y.from) * (x.to - x.from) > 0.0)
        .then_some(turns)
}

/// Two curved strips that are one region (split where the other profile's
/// pieces start) as one strip.
fn joined(s: &Strip, t: &Strip) -> Option<Strip> {
    if s.flat() || t.flat() {
        return None;
    }
    let side = |a: Side, b: Side| -> Option<Side> {
        match (a, b) {
            (Side::Point(p), Side::Point(q)) if near(p, q) => Some(Side::Point(p)),
            (Side::Arc(x), Side::Arc(y)) => runs_on(&x, &y).map(|turns| {
                Side::Arc(ArcPiece {
                    to: y.to + turns,
                    ..x
                })
            }),
            _ => None,
        }
    };
    Some(Strip {
        a: side(s.a, t.a)?,
        b: side(s.b, t.b)?,
    })
}

/// The strips with each region about a corner or an arc in one piece, a
/// closed transition's last and first too.
fn merged(strips: Vec<Strip>, closed: bool) -> Vec<Strip> {
    let mut out: Vec<Strip> = Vec::new();
    for s in strips {
        if let Some(last) = out.last_mut()
            && let Some(both) = joined(last, &s)
        {
            *last = both;
        } else {
            out.push(s);
        }
    }
    if closed
        && out.len() > 1
        && let Some(both) = joined(&out[out.len() - 1], &out[0])
    {
        out[0] = both;
        out.pop();
    }
    out
}

fn end_point(e: &Element, _k: usize) -> V3 {
    match *e {
        Element::Line { b, .. } => b,
        Element::Turn { point, arc, .. } => arc.map_or(point, |a| a.point(1.0)),
    }
}

/// How many facets a press-brake region about an arc of `radius` turning
/// `angle` gets: `count`, or as few as keep the chords within `tolerance`.
pub(super) fn facet_count(
    count: Option<usize>,
    tolerance: Option<f64>,
    radius: f64,
    angle: f64,
) -> usize {
    match (count, tolerance) {
        (_, Some(tol)) => {
            if tol >= radius {
                return 1;
            }
            let step = 2.0 * (1.0 - tol / radius).acos();
            ((angle.abs() / step).ceil() as usize).clamp(1, 1000)
        }
        (Some(n), None) => n.max(1),
        (None, None) => 4,
    }
}

/// The strips with every cone and arc region cut into flat facets.
pub(super) fn faceted(
    strips: &[Strip],
    count: Option<usize>,
    tolerance: Option<f64>,
) -> Vec<Strip> {
    let mut out = Vec::new();
    for s in strips {
        match (s.a, s.b) {
            (Side::Arc(x), Side::Arc(y)) => {
                let n = facet_count(count, tolerance, x.radius, x.to - x.from).max(facet_count(
                    count,
                    tolerance,
                    y.radius,
                    y.to - y.from,
                ));
                for (cx, cy) in x.chords(n).into_iter().zip(y.chords(n)) {
                    out.push(Strip {
                        a: Side::Segment(cx.0, cx.1),
                        b: Side::Segment(cy.0, cy.1),
                    });
                }
            }
            (Side::Arc(x), other) => {
                let n = facet_count(count, tolerance, x.radius, x.to - x.from);
                for c in x.chords(n) {
                    out.push(Strip {
                        a: Side::Segment(c.0, c.1),
                        b: other,
                    });
                }
            }
            (other, Side::Arc(y)) => {
                let n = facet_count(count, tolerance, y.radius, y.to - y.from);
                for c in y.chords(n) {
                    out.push(Strip {
                        a: other,
                        b: Side::Segment(c.0, c.1),
                    });
                }
            }
            _ => out.push(*s),
        }
    }
    out
}

/// A profile cut into a polyline (arcs and other curves into chords, as
/// facets are counted): its points in order, a closed one without its
/// last point again.
pub(super) fn polyline(chain: &Chain, count: Option<usize>, tolerance: Option<f64>) -> Vec<V3> {
    let mut out = Vec::new();
    for p in &chain.pieces {
        let n = match p {
            Piece::Line(..) => 1,
            Piece::Arc(a) => facet_count(count, tolerance, a.radius, a.to - a.from),
            Piece::Other { .. } => match tolerance {
                None => count.unwrap_or(4).max(1),
                Some(tol) => {
                    // As few chords as stay within the tolerance at their
                    // middles.
                    let mut n = 1;
                    while n < 1000 {
                        let off = (0..n).any(|i| {
                            let (s0, s1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
                            let mid = p.point((s0 + s1) / 2.0);
                            let chord = lerp3(p.point(s0), p.point(s1), 0.5);
                            distance(mid, chord) > tol
                        });
                        if !off {
                            break;
                        }
                        n *= 2;
                    }
                    n
                }
            },
        };
        let s = p.samples(n);
        out.extend_from_slice(&s[..n]);
    }
    if !chain.closed
        && let Some(last) = chain.pieces.last()
    {
        out.push(last.end());
    }
    out
}

/// Triangles between two polylines (open, in the same direction), each
/// time across the shorter diagonal.
pub(super) fn triangulated(a: &[V3], b: &[V3]) -> Vec<Strip> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i + 1 < a.len() || j + 1 < b.len() {
        let advance_a = if i + 1 >= a.len() {
            false
        } else if j + 1 >= b.len() {
            true
        } else {
            distance(a[i + 1], b[j]) <= distance(a[i], b[j + 1])
        };
        if advance_a {
            out.push(Strip {
                a: Side::Segment(a[i], a[i + 1]),
                b: Side::Point(b[j]),
            });
            i += 1;
        } else {
            out.push(Strip {
                a: Side::Point(a[i]),
                b: Side::Segment(b[j], b[j + 1]),
            });
            j += 1;
        }
    }
    out
}

/// A closed polyline turned to start at the point at `at` along it (a new
/// vertex where it falls inside a segment), closed again at its end.
pub(super) fn polyline_from(points: &[V3], at: f64) -> Vec<V3> {
    let n = points.len();
    let mut walked = 0.0;
    let (mut i, mut s) = (0, 0.0);
    for k in 0..n {
        let len = distance(points[k], points[(k + 1) % n]);
        if walked + len >= at || k + 1 == n {
            i = k;
            s = if len > 0.0 {
                ((at - walked) / len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            break;
        }
        walked += len;
    }
    let start = lerp3(points[i], points[(i + 1) % n], s);
    let mut out = vec![start];
    for k in 1..=n {
        let v = points[(i + k) % n];
        let last = *out.last().expect("a point");
        if !near(v, last) && !(k == n && near(v, start)) {
            out.push(v);
        }
    }
    if !near(*out.last().expect("a point"), start) || out.len() == 1 {
        out.push(start);
    }
    out
}

/// Where along a closed polyline a point lies nearest (its length from the
/// start), and how far it is.
pub(super) fn nearest_on(points: &[V3], p: V3) -> (f64, f64) {
    let n = points.len();
    let mut best = (0.0, f64::INFINITY);
    let mut walked = 0.0;
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let d = sub(b, a);
        let len2 = dot(d, d);
        let s = if len2 > 0.0 {
            (dot(sub(p, a), d) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dist = distance(lerp3(a, b, s), p);
        if dist < best.1 {
            best = (walked + s * len2.sqrt(), dist);
        }
        walked += len2.sqrt();
    }
    best
}

/// The length of a closed polyline up to each vertex.
pub(super) fn polyline_length(points: &[V3], closed: bool) -> f64 {
    let n = points.len();
    let m = if closed { n } else { n.saturating_sub(1) };
    (0..m)
        .map(|i| distance(points[i], points[(i + 1) % n]))
        .sum()
}

/// The ruling of a closed transition's strips that the seam runs along: in
/// strip `strip` at `s`. With `at` (a length along the first profile), the
/// ruling through that point (the one after a corner's fan); without, the
/// shortest.
pub(super) fn seam_place(strips: &[Strip], at: Option<f64>) -> (usize, f64) {
    match at {
        Some(at) => {
            let mut walked = 0.0;
            for (k, s) in strips.iter().enumerate() {
                let len = s.a.length();
                if len <= NEAR {
                    continue;
                }
                if walked + len > at + NEAR || k + 1 == strips.len() {
                    let f = ((at - walked) / len).clamp(0.0, 1.0);
                    return (k, f);
                }
                walked += len;
            }
            (0, 0.0)
        }
        None => {
            let mut best = (0, 0.0, f64::INFINITY);
            for (k, s) in strips.iter().enumerate() {
                let n = 32;
                let length = |f: f64| {
                    let (a, b) = s.ruling(f);
                    distance(a, b)
                };
                for i in 0..=n {
                    let f = i as f64 / n as f64;
                    let l = length(f);
                    if l < best.2 - 1e-9 {
                        best = (k, f, l);
                    }
                }
                // Refined about the best sample of this strip.
                if best.0 == k {
                    let (mut lo, mut hi) = (
                        (best.1 - 1.0 / n as f64).max(0.0),
                        (best.1 + 1.0 / n as f64).min(1.0),
                    );
                    for _ in 0..60 {
                        let (m1, m2) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
                        if length(m1) <= length(m2) {
                            hi = m2;
                        } else {
                            lo = m1;
                        }
                    }
                    let f = (lo + hi) / 2.0;
                    if length(f) < best.2 {
                        best = (k, f, length(f));
                    }
                }
            }
            (best.0, best.1)
        }
    }
}

/// The closed transition's strips opened at the seam (strip `strip` at
/// `s`): they run from the ruling after it round to the ruling before it.
pub(super) fn opened_at(strips: &[Strip], strip: usize, s: f64) -> Vec<Strip> {
    let n = strips.len();
    let here = strips[strip];
    let mut out = Vec::with_capacity(n + 1);
    if s < 1.0 - 1e-12 {
        out.push(here.part(s, 1.0));
    }
    for k in 1..n {
        out.push(strips[(strip + k) % n]);
    }
    if s > 1e-12 {
        out.push(here.part(0.0, s));
    }
    out.retain(|x| !(x.a.is_point() && x.b.is_point()));
    out
}

/// Where a length `along` a side of the strips falls: the strip and the
/// fraction of its side, past any strips whose side is a point.
fn locate(strips: &[Strip], along: f64, side: fn(&Strip) -> Side) -> Option<(usize, f64)> {
    let mut walked = 0.0;
    for (k, s) in strips.iter().enumerate() {
        let len = side(s).length();
        if len <= NEAR {
            continue;
        }
        if walked + len > along + 1e-9 {
            return Some((k, ((along - walked) / len).clamp(0.0, 1.0)));
        }
        walked += len;
    }
    None
}

/// The strips with `la` taken from the start of their first sides and `lb`
/// from the start of their second sides (half the gap each side of a
/// seam). The new first ruling joins the two cut points; where they fall
/// in different strips, triangles and cones from the farther one fill the
/// part between.
pub(super) fn trimmed_start(strips: &[Strip], la: f64, lb: f64) -> Result<Vec<Strip>, String> {
    trimmed_start_with(strips, la, lb, true)
}

/// [`trimmed_start`]; without `fill`, two arcs' region is cut at the
/// farther of the two cut points on both arcs (no cone from it fills the
/// rest: its facets would fan into that point, where bends cannot meet,
/// mitcad#2560).
pub(super) fn trimmed_start_with(
    strips: &[Strip],
    la: f64,
    lb: f64,
    fill: bool,
) -> Result<Vec<Strip>, String> {
    let too_long = || "the seam's gap is longer than a profile".to_owned();
    let (i, sa) = locate(strips, la, |s| s.a).ok_or_else(too_long)?;
    let (j, sb) = locate(strips, lb, |s| s.b).ok_or_else(too_long)?;
    let mut out = Vec::new();
    let rest = |from: usize| strips[from + 1..].to_vec();
    if i == j {
        let s = strips[i];
        if s.matched() && (sa - sb).abs() > 1e-12 && !fill {
            out.push(s.part(sa.max(sb), 1.0));
        } else if s.matched() && (sa - sb).abs() > 1e-12 {
            let f = sa.max(sb);
            if sb > sa {
                // From B's cut point back over A.
                out.push(Strip {
                    a: s.a.part(sa, f),
                    b: Side::Point(s.b.point(f)),
                });
            } else {
                out.push(Strip {
                    a: Side::Point(s.a.point(f)),
                    b: s.b.part(sb, f),
                });
            }
            out.push(s.part(f, 1.0));
        } else {
            out.push(Strip {
                a: s.a.part(sa, 1.0),
                b: s.b.part(sb, 1.0),
            });
        }
        out.extend(rest(i));
    } else if i < j {
        // B's cut point is farther on: a fan from it over A up to strip j.
        let apex = strips[j].b.point(sb);
        let mut fan = vec![strips[i].a.part(sa, 1.0)];
        fan.extend(strips[i + 1..j].iter().map(|s| s.a));
        let s = strips[j];
        let after = if s.matched() {
            fan.push(s.a.part(0.0, sb));
            s.part(sb, 1.0)
        } else {
            Strip {
                a: s.a,
                b: s.b.part(sb, 1.0),
            }
        };
        out.extend(
            fan.into_iter()
                .filter(|side| !side.is_point())
                .map(|side| Strip {
                    a: side,
                    b: Side::Point(apex),
                }),
        );
        out.push(after);
        out.extend(rest(j));
    } else {
        // A's cut point is farther on: a fan from it over B up to strip i.
        let apex = strips[i].a.point(sa);
        let mut fan = vec![strips[j].b.part(sb, 1.0)];
        fan.extend(strips[j + 1..i].iter().map(|s| s.b));
        let s = strips[i];
        let after = if s.matched() {
            fan.push(s.b.part(0.0, sa));
            s.part(sa, 1.0)
        } else {
            Strip {
                a: s.a.part(sa, 1.0),
                b: s.b,
            }
        };
        out.extend(
            fan.into_iter()
                .filter(|side| !side.is_point())
                .map(|side| Strip {
                    a: Side::Point(apex),
                    b: side,
                }),
        );
        out.push(after);
        out.extend(rest(i));
    }
    out.retain(|x| !(x.a.is_point() && x.b.is_point()));
    if out.is_empty() {
        return Err("the seam's gap leaves no material".into());
    }
    Ok(out)
}

/// [`trimmed_start`] at the end of the strips.
pub(super) fn trimmed_end(strips: &[Strip], la: f64, lb: f64) -> Result<Vec<Strip>, String> {
    trimmed_end_with(strips, la, lb, true)
}

/// [`trimmed_end`], filling as [`trimmed_start_with`].
pub(super) fn trimmed_end_with(
    strips: &[Strip],
    la: f64,
    lb: f64,
    fill: bool,
) -> Result<Vec<Strip>, String> {
    let reversed: Vec<Strip> = strips.iter().rev().map(Strip::reversed).collect();
    Ok(trimmed_start_with(&reversed, la, lb, fill)?
        .iter()
        .rev()
        .map(Strip::reversed)
        .collect())
}

/// The mitre between two unit normals: the direction whose dot product
/// with both is one.
fn mitre(n1: V3, n2: V3) -> V3 {
    scale(add(n1, n2), 1.0 / (1.0 + dot(n1, n2)))
}

/// How two strips meet at their shared ruling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Join {
    /// The fold's angle, positive towards the back (0 where they meet
    /// smoothly).
    pub angle: f64,
    /// The fold's inside is on the front.
    pub inside_front: bool,
}

/// The transition: its strips in order, their normals, the material
/// direction (`side` times the normals) and how they meet.
#[derive(Debug, Clone)]
pub(super) struct Transition {
    pub strips: Vec<Strip>,
    /// 1 or -1: the material lies along `side` times the normals.
    pub side: f64,
    /// Between strip k and k + 1.
    pub joins: Vec<Join>,
    /// Each strip's neutral surface lies this much farther behind the
    /// front than the common neutral depth: a bend whose inside is on the
    /// back develops at the K-factor from its inside (mitcad#2560); 0
    /// elsewhere.
    pub shift: Vec<f64>,
}

/// An error about a region of the transition: its points, for a place.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RegionError {
    pub message: String,
    pub points: Vec<V3>,
}

impl RegionError {
    fn new(message: impl Into<String>, strip: &Strip) -> Self {
        Self {
            message: message.into(),
            points: strip.points(),
        }
    }
}

/// The transition of the strips with the material outwards (a closed
/// one) or along the normals (an open one), the other way with `flip`.
pub(super) fn transition(
    strips: Vec<Strip>,
    closed: bool,
    flip: bool,
) -> Result<Transition, RegionError> {
    have_normals(&strips)?;
    let mut side = 1.0;
    if closed {
        let all: Vec<V3> = strips.iter().flat_map(Strip::points).collect();
        let centre = scale(
            all.iter().fold([0.0; 3], |acc, p| add(acc, *p)),
            1.0 / all.len().max(1) as f64,
        );
        let mid = &strips[strips.len() / 2];
        let (a, b) = mid.ruling(0.5);
        let n = mid.normal(0.5).expect("checked");
        if dot(n, sub(lerp3(a, b, 0.5), centre)) < 0.0 {
            side = -1.0;
        }
    }
    if flip {
        side = -side;
    }
    meeting(strips, side)
}

/// Every strip has a normal at its start, middle and end.
fn have_normals(strips: &[Strip]) -> Result<(), RegionError> {
    for s in strips {
        for f in [0.0, 0.5, 1.0] {
            if s.normal(f).is_none() {
                return Err(RegionError::new(
                    "the profiles meet here: a lofted flange needs room between them",
                    s,
                ));
            }
        }
    }
    Ok(())
}

/// The transition of the strips with the material along `side` times
/// their normals: how they meet.
fn meeting(strips: Vec<Strip>, side: f64) -> Result<Transition, RegionError> {
    have_normals(&strips)?;
    let mut joins = Vec::new();
    for w in strips.windows(2) {
        // A bend is tangent to the facets beside it.
        if w[0].bend() || w[1].bend() {
            joins.push(Join {
                angle: 0.0,
                inside_front: false,
            });
            continue;
        }
        let n1 = w[0].normal(1.0).expect("checked");
        let n2 = w[1].normal(0.0).expect("checked");
        // Exact for slight folds too (where an arc cosine is not).
        let angle = norm(cross(n1, n2)).atan2(dot(n1, n2));
        if angle <= FLAT {
            joins.push(Join {
                angle: 0.0,
                inside_front: false,
            });
            continue;
        }
        if angle >= SHARPEST {
            return Err(RegionError::new(
                format!(
                    "the transition folds back on itself here ({:.1} degrees): move the profiles \
                     apart or change their shapes",
                    angle.to_degrees()
                ),
                &w[1],
            ));
        }
        let (a, b) = w[1].ruling(0.0);
        let r = unit3(sub(b, a)).expect("checked");
        let mut into = cross(n2, r);
        let (p, q) = w[1].ruling(0.5);
        if dot(into, sub(lerp3(p, q, 0.5), lerp3(a, b, 0.5))) < 0.0 {
            into = scale(into, -1.0);
        }
        let towards_back = dot(into, scale(n1, side)) > 0.0;
        joins.push(Join {
            angle: if towards_back { angle } else { -angle },
            inside_front: !towards_back,
        });
    }
    let shift = vec![0.0; strips.len()];
    Ok(Transition {
        strips,
        side,
        joins,
        shift,
    })
}

/// A bend of the press brake's folds (mitcad#2560): its strip in the bent
/// transition, the fold it rounds and its inside radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Bend {
    pub strip: usize,
    pub join: Join,
    pub radius: f64,
}

// Corner reliefs where bends meet in a point (mitcad#2576).
#[path = "lofted_relief.rs"]
mod relief;
use relief::{Edge, Outline};

/// How much larger than the least size a corner relief must be (mm), so
/// that its cut keeps a length between the bends' tangent lines.
const SPARE: f64 = 0.01;

/// A fold's bend before it is placed (mitcad#2560): its cylinder and the
/// facets' setbacks.
#[derive(Debug, Clone, Copy)]
struct Fold {
    /// Its ends on the first and the second profile.
    ends: [V3; 2],
    /// Its unit direction from the first profile to the second: the
    /// cylinder's axis.
    axis: V3,
    /// Unit directions into the facet before it and the one after it,
    /// across it.
    into: [V3; 2],
    setback: f64,
    centre: V3,
    p: V3,
    q: V3,
    /// The front's radius.
    radius: f64,
    to: f64,
    /// Its neutral surface's shift ([`Transition::shift`]).
    shift: f64,
}

impl Fold {
    /// A point of its tangent line on the facet before it (0) or after it
    /// (1): the cylinder's line at its start or its end.
    fn tangent(&self, side: usize) -> V3 {
        add(self.ends[0], scale(self.into[side], self.setback))
    }

    /// The facet before it (0) or after it (1) cut back to it is kept on
    /// the side of this plane its normal points to.
    fn setback_plane(&self, side: usize) -> Plane {
        (self.tangent(side), self.into[side])
    }

    /// Where its tangent line on a side meets a plane.
    fn met(&self, side: usize, (origin, normal): Plane) -> Option<V3> {
        let start = self.tangent(side);
        let along = dot(self.axis, normal);
        (along.abs() > 1e-9).then(|| {
            add(
                start,
                scale(self.axis, dot(sub(origin, start), normal) / along),
            )
        })
    }

    fn section(&self, plane: Plane) -> Section {
        Section {
            centre: self.centre,
            axis: self.axis,
            p: self.p,
            q: self.q,
            radius: self.radius,
            from: 0.0,
            to: self.to,
            plane,
        }
    }
}

/// The press brake's transition with its folds made bends of the inside
/// `radius` (mitcad#2560): at each fold a cylinder strip tangent to the
/// facets beside it, the facets cut back to where it meets them (the
/// setback, `R tan(θ / 2)` for the front's radius R), each bend's ends
/// where its cylinder meets the profiles' `planes`. The front's radius is
/// `radius` where the fold's inside is on the front, `radius + thickness`
/// where it is on the back; the bend develops at the K-factor `k` from its
/// inside. Where folds meet in a point of a profile (a fan's tip) a corner
/// relief of the size `relief` opens about it (mitcad#2576,
/// `lofted_relief.rs`): the facets meeting there are cut straight across
/// `relief` from it, and the bends end on those cuts, short of the point.
pub(super) fn bent(
    t: &Transition,
    radius: f64,
    thickness: f64,
    k: f64,
    planes: [Plane; 2],
    relief: f64,
) -> Result<(Transition, Vec<Bend>), RegionError> {
    let n = t.strips.len();
    // The bend at each fold.
    let mut folds: Vec<Option<Fold>> = vec![None; t.joins.len()];
    for (i, join) in t.joins.iter().enumerate() {
        if join.angle == 0.0 {
            continue;
        }
        let (s1, s2) = (t.strips[i], t.strips[i + 1]);
        if !s1.flat() || !s2.flat() {
            return Err(RegionError::new(
                "only the flat facets of a press brake transition bend: this fold meets a formed \
                 region",
                &s2,
            ));
        }
        let (oa, ob) = s1.ruling(1.0);
        let e = unit3(sub(ob, oa))
            .ok_or_else(|| RegionError::new("the profiles meet at this fold", &s2))?;
        for (_, normal) in planes {
            if dot(e, normal).abs() < 1e-6 {
                return Err(RegionError::new(
                    "this fold runs along a profile's plane: it has no bend between the profiles",
                    &s2,
                ));
            }
        }
        // Into each facet from the fold, across it.
        let into = |s: &Strip| -> Option<V3> {
            let c = scale(
                add(
                    add(s.a.point(0.0), s.a.point(1.0)),
                    add(s.b.point(0.0), s.b.point(1.0)),
                ),
                0.25,
            );
            let d = sub(c, oa);
            unit3(sub(d, scale(e, dot(d, e))))
        };
        let (Some(u1), Some(u2)) = (into(&s1), into(&s2)) else {
            return Err(RegionError::new(
                "a facet beside this fold has no width",
                &s2,
            ));
        };
        let theta = join.angle.abs();
        // The fold's inside, between the facets.
        let w = unit3(add(u1, u2))
            .ok_or_else(|| RegionError::new("the facets beside this fold lie flat", &s2))?;
        let material = scale(s1.normal(0.5).expect("checked"), t.side);
        let outside_front = dot(material, w) > 0.0;
        let rho = if outside_front {
            radius + thickness
        } else {
            radius
        };
        let shift = if outside_front {
            (1.0 - 2.0 * k) * thickness
        } else {
            0.0
        };
        let setback = rho * (theta / 2.0).tan();
        let centre = add(oa, scale(w, rho / (theta / 2.0).cos()));
        let first = sub(add(oa, scale(u1, setback)), centre);
        let last = sub(add(oa, scale(u2, setback)), centre);
        let (Some(p), Some(r2)) = (unit3(first), unit3(last)) else {
            return Err(RegionError::new("this fold's bend has no radius", &s2));
        };
        let q = unit3(sub(r2, scale(p, dot(r2, p))))
            .ok_or_else(|| RegionError::new("this fold has no angle", &s2))?;
        let to = dot(r2, q).atan2(dot(r2, p));
        folds[i] = Some(Fold {
            ends: [oa, ob],
            axis: e,
            into: [u1, u2],
            setback,
            centre,
            p,
            q,
            radius: rho,
            to,
            shift,
        });
    }
    let narrow = |s: &Strip| {
        RegionError::new(
            "this facet is too narrow for the bends on its sides: use fewer facets or a smaller \
             bend radius",
            s,
        )
    };
    let large = |s: &Strip| {
        RegionError::new(
            "the corner relief is too large here: its cut reaches past the facets and bends that \
             meet in this point. Make it smaller, or use fewer facets",
            s,
        )
    };
    let formed = |s: &Strip| {
        RegionError::new(
            "only the flat facets of a press brake transition bend: this fold meets a formed \
             region",
            s,
        )
    };
    let before = |s: usize| s.checked_sub(1).and_then(|i| folds[i]);
    let after = |s: usize| folds.get(s).copied().flatten();
    // The tips where folds with bends meet in a point, and each facet's cut
    // about those it meets (mitcad#2576).
    let bent_at: Vec<bool> = folds.iter().map(Option::is_some).collect();
    let tips = relief::tips(&t.strips, &bent_at);
    let mut cuts: Vec<Vec<relief::Cut>> = vec![Vec::new(); n];
    for (index, tip) in tips.iter().enumerate() {
        for &s in &tip.strips {
            let strip = &t.strips[s];
            if !strip.flat() {
                return Err(formed(strip));
            }
            let cut =
                relief::facet_cut(strip, index, tip.point, relief).ok_or_else(|| large(strip))?;
            cuts[s].push(cut);
        }
    }
    // Each facet beside a bend or a tip as its outline, cut back to the
    // bends beside it.
    let mut outlines: Vec<Option<Outline>> = Vec::with_capacity(n);
    for (s, strip) in t.strips.iter().enumerate() {
        if before(s).is_none() && after(s).is_none() && cuts[s].is_empty() {
            outlines.push(None);
            continue;
        }
        let mut o = relief::outline_of(strip);
        if let Some(f) = before(s) {
            o = relief::clipped(&o, f.setback_plane(1), Edge::Start);
        }
        if let Some(f) = after(s) {
            o = relief::clipped(&o, f.setback_plane(0), Edge::End);
        }
        outlines.push(Some(o));
    }
    // Every bend through a tip must reach the relief's cut on both facets
    // beside it: the end of each tangent line towards the tip is cut away.
    let mut needed: Option<(f64, usize)> = None;
    for (index, tip) in tips.iter().enumerate() {
        let end = usize::from(tip.second);
        for &s in &tip.strips {
            let (Some(o), Some(cut)) = (&outlines[s], cuts[s].iter().find(|c| c.tip == index))
            else {
                continue;
            };
            for (edge, fold) in [(Edge::Start, before(s)), (Edge::End, after(s))] {
                let Some(f) = fold else {
                    continue;
                };
                if !near(f.ends[end], tip.point) {
                    continue;
                }
                if let Some(v) = relief::edge_end(o, edge, tip.second) {
                    let size = cut.size_through(tip.point, v);
                    if needed.is_none_or(|(most, _)| size > most) {
                        needed = Some((size, s));
                    }
                }
            }
        }
    }
    if let Some((size, s)) = needed
        && relief < size + SPARE
    {
        let least = ((size + SPARE) * 10.0).ceil() / 10.0;
        return Err(RegionError::new(
            format!(
                "the corner relief where these bends meet in a point is too small: they would run \
                 into each other before its cut. Make it at least {:.1} mm",
                least
            ),
            &t.strips[s],
        ));
    }
    for (s, o) in outlines.iter_mut().enumerate() {
        if let Some(o) = o {
            for cut in &cuts[s] {
                let edge = if tips[cut.tip].second {
                    Edge::Second
                } else {
                    Edge::First
                };
                *o = relief::clipped(o, cut.plane, edge);
            }
        }
    }
    // The facets' corners along both profiles' sides (and the reliefs'
    // cuts there).
    let mut sides: Vec<Option<(Vec<V3>, Vec<V3>)>> = Vec::with_capacity(n);
    for (s, strip) in t.strips.iter().enumerate() {
        let Some(o) = &outlines[s] else {
            sides.push(None);
            continue;
        };
        if !strip.flat() {
            return Err(formed(strip));
        }
        let refused = || {
            if cuts[s].is_empty() {
                narrow(strip)
            } else {
                large(strip)
            }
        };
        let (first, second) = relief::chains(o).ok_or_else(refused)?;
        // A side with a length keeps one.
        for (side, chain) in [(strip.a, &first), (strip.b, &second)] {
            if !side.is_point() && relief::length_of(chain) <= NEAR {
                return Err(refused());
            }
        }
        sides.push(Some((first, second)));
    }
    // Each bend's ends on the profiles' planes or on a relief's cuts,
    // where the facets beside it end.
    let mut made: Vec<Option<Strip>> = vec![None; t.joins.len()];
    for (i, fold) in folds.iter().enumerate() {
        let Some(f) = fold else {
            continue;
        };
        let s2 = &t.strips[i + 1];
        let refused = || {
            if cuts[i].is_empty() && cuts[i + 1].is_empty() {
                narrow(s2)
            } else {
                large(s2)
            }
        };
        let end_plane = |end: usize| -> Option<Plane> {
            let Some(index) = tips
                .iter()
                .position(|tip| usize::from(tip.second) == end && near(tip.point, f.ends[end]))
            else {
                return Some(planes[end]);
            };
            let cut_of = |s: usize| cuts[s].iter().find(|c| c.tip == index).map(|c| c.plane);
            relief::bend_end(f.met(0, cut_of(i)?)?, f.met(1, cut_of(i + 1)?)?, f.axis)
        };
        let (Some(plane_a), Some(plane_b)) = (end_plane(0), end_plane(1)) else {
            return Err(refused());
        };
        let (a, b) = (f.section(plane_a), f.section(plane_b));
        // Its lines run from the first profile's side to the second's.
        if [0.0, 0.5, 1.0]
            .iter()
            .any(|&x| dot(sub(b.point(x), a.point(x)), f.axis) <= NEAR)
        {
            return Err(refused());
        }
        // It ends where the facets beside it end: the same points.
        let fit = |chain: &mut Vec<V3>, last: bool, p: V3| {
            let at = if last { chain.len() - 1 } else { 0 };
            let fits = distance(chain[at], p) <= NEAR;
            if fits {
                chain[at] = p;
            }
            fits
        };
        let (here, next) = sides.split_at_mut(i + 1);
        let fits = match (here[i].as_mut(), next[0].as_mut()) {
            (Some((a1, b1)), Some((a2, b2))) => {
                fit(a1, true, a.point(0.0))
                    && fit(b1, true, b.point(0.0))
                    && fit(a2, false, a.point(1.0))
                    && fit(b2, false, b.point(1.0))
            }
            _ => false,
        };
        if !fits {
            return Err(refused());
        }
        made[i] = Some(Strip {
            a: Side::Section(a),
            b: Side::Section(b),
        });
    }
    let mut strips = Vec::with_capacity(2 * n);
    let mut shift = Vec::with_capacity(2 * n);
    let mut bends = Vec::new();
    for (s, strip) in t.strips.iter().enumerate() {
        match &sides[s] {
            None => {
                strips.push(*strip);
                shift.push(0.0);
            }
            Some((first, second)) => {
                for piece in relief::strips_between(first, second) {
                    strips.push(piece);
                    shift.push(0.0);
                }
            }
        }
        if let (Some(bend), Some(f)) = (made.get(s).copied().flatten(), after(s)) {
            bends.push(Bend {
                strip: strips.len(),
                join: t.joins[s],
                radius,
            });
            strips.push(bend);
            shift.push(f.shift);
        }
    }
    let mut out = meeting(strips, t.side)?;
    out.shift = shift;
    Ok((out, bends))
}

/// A band of a surface `h` behind the front: a flat quadrilateral
/// (`[a0, a1, b0, b1]`, a triangle where a side is a point) or a curved
/// strip moved along its normals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Band {
    Flat {
        corners: [V3; 4],
        normal: V3,
        /// The strip it belongs to.
        strip: usize,
    },
    Curved {
        strip: usize,
    },
}

impl Band {
    pub fn strip(&self) -> usize {
        match self {
            Self::Flat { strip, .. } | Self::Curved { strip } => *strip,
        }
    }
}

impl Transition {
    fn mitre_before(&self, k: usize) -> V3 {
        let n = self.strips[k].normal(0.0).expect("checked");
        if k == 0 {
            return n;
        }
        mitre(self.strips[k - 1].normal(1.0).expect("checked"), n)
    }

    fn mitre_after(&self, k: usize) -> V3 {
        let n = self.strips[k].normal(1.0).expect("checked");
        if k + 1 == self.strips.len() {
            return n;
        }
        mitre(n, self.strips[k + 1].normal(0.0).expect("checked"))
    }

    /// The bands of the surface at any depth: the same for every depth (a
    /// curved strip's slivers at its folds are there even at the front,
    /// where they have no width).
    pub fn bands(&self) -> Vec<Band> {
        let mut out = Vec::new();
        for (k, s) in self.strips.iter().enumerate() {
            if s.flat() {
                out.push(Band::Flat {
                    corners: [
                        s.a.point(0.0),
                        s.a.point(1.0),
                        s.b.point(0.0),
                        s.b.point(1.0),
                    ],
                    normal: s.normal(0.5).expect("checked"),
                    strip: k,
                });
                continue;
            }
            let (n0, n1) = (
                s.normal(0.0).expect("checked"),
                s.normal(1.0).expect("checked"),
            );
            let (m0, m1) = (self.mitre_before(k), self.mitre_after(k));
            // A fold too slight to measure leaves no sliver (its width
            // below a hundredth of a micrometre per millimetre of depth).
            if distance(m0, n0) > 1e-8 {
                out.push(Band::Flat {
                    corners: [s.a.point(0.0); 4],
                    normal: n0,
                    strip: k,
                });
            }
            out.push(Band::Curved { strip: k });
            if distance(m1, n1) > 1e-8 {
                out.push(Band::Flat {
                    corners: [s.a.point(1.0); 4],
                    normal: n1,
                    strip: k,
                });
            }
        }
        out
    }

    /// The four corners of a band `depth` behind the front.
    fn flat_at(&self, bands: &[Band], index: usize, depth: f64) -> [V3; 4] {
        let Band::Flat { strip, .. } = bands[index] else {
            unreachable!("a flat band");
        };
        let s = &self.strips[strip];
        let off = |p: V3, m: V3| add(p, scale(m, depth * self.side));
        if s.flat() {
            let (m0, m1) = (self.mitre_before(strip), self.mitre_after(strip));
            return [
                off(s.a.point(0.0), m0),
                off(s.a.point(1.0), m1),
                off(s.b.point(0.0), m0),
                off(s.b.point(1.0), m1),
            ];
        }
        // A sliver: before the curved band (at its start) or after it.
        let before = matches!(bands.get(index + 1), Some(Band::Curved { strip: k }) if *k == strip);
        if before {
            let (m, n) = (self.mitre_before(strip), s.normal(0.0).expect("checked"));
            let (a, b) = s.ruling(0.0);
            [off(a, m), off(a, n), off(b, m), off(b, n)]
        } else {
            let (n, m) = (s.normal(1.0).expect("checked"), self.mitre_after(strip));
            let (a, b) = s.ruling(1.0);
            [off(a, n), off(a, m), off(b, n), off(b, m)]
        }
    }

    /// A curved strip's ruling at `f`, `depth` behind the front.
    fn curved_at(&self, strip: usize, f: f64, depth: f64) -> (V3, V3) {
        let s = &self.strips[strip];
        let n = s.normal(f).expect("checked");
        let (a, b) = s.ruling(f);
        let d = scale(n, depth * self.side);
        (add(a, d), add(b, d))
    }

    /// Its derivative by `f` (central differences).
    fn curved_derivative(&self, strip: usize, f: f64, depth: f64) -> (V3, V3) {
        let e = 1e-6;
        let (f0, f1) = ((f - e).max(0.0), (f + e).min(1.0));
        let (a0, b0) = self.curved_at(strip, f0, depth);
        let (a1, b1) = self.curved_at(strip, f1, depth);
        let w = 1.0 / (f1 - f0);
        (scale(sub(a1, a0), w), scale(sub(b1, b0), w))
    }

    /// The material does not fold over itself `depth` behind the front:
    /// every band's sides there run on the way the front's do.
    pub fn check_depth(&self, depth: f64) -> Result<(), RegionError> {
        let bands = self.bands();
        let over = |k: usize| {
            RegionError::new(
                "the material folds over itself here, the thickness behind such a sharp corner \
                 of the surface: put the material on the other side (Flip), make the sheet \
                 thinner or round the profiles' corners",
                &self.strips[k],
            )
        };
        for (i, band) in bands.iter().enumerate() {
            let k = band.strip();
            let s = &self.strips[k];
            match band {
                Band::Flat { normal, .. } => {
                    let c = self.flat_at(&bands, i, depth);
                    let front = self.flat_at(&bands, i, 0.0);
                    // Each moved side runs the way its front side does, or
                    // across where that is a point.
                    for (moved, was) in [
                        ((c[0], c[1]), (front[0], front[1])),
                        ((c[2], c[3]), (front[2], front[3])),
                    ] {
                        let d = sub(moved.1, moved.0);
                        let w = sub(was.1, was.0);
                        if norm(w) > NEAR && dot(d, w) < -1e-9 {
                            return Err(over(k));
                        }
                    }
                    let along = sub(lerp3(c[1], c[3], 0.5), lerp3(c[0], c[2], 0.5));
                    let r = sub(lerp3(c[2], c[3], 0.5), lerp3(c[0], c[1], 0.5));
                    if norm(along) > NEAR && dot(cross(r, along), *normal) < -1e-9 {
                        return Err(over(k));
                    }
                }
                Band::Curved { strip } => {
                    for step in 0..SPANS {
                        let f0 = step as f64 / SPANS as f64;
                        let f1 = (step + 1) as f64 / SPANS as f64;
                        let (a0, b0) = self.curved_at(*strip, f0, depth);
                        let (a1, b1) = self.curved_at(*strip, f1, depth);
                        let t = s.along((f0 + f1) / 2.0);
                        if dot(sub(a1, a0), t) < -1e-9 || dot(sub(b1, b0), t) < -1e-9 {
                            return Err(over(k));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// A band laid flat: a flat band's four corners, or a curved band's
/// rulings at `SPANS + 1` stations with their derivatives by the fraction.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Developed {
    Flat([P2; 4]),
    Curved {
        rulings: Vec<(P2, P2)>,
        derivatives: Vec<(P2, P2)>,
    },
}

impl Developed {
    pub fn start(&self) -> (P2, P2) {
        match self {
            Self::Flat(c) => (c[0], c[2]),
            Self::Curved { rulings, .. } => rulings[0],
        }
    }

    pub fn end(&self) -> (P2, P2) {
        match self {
            Self::Flat(c) => (c[1], c[3]),
            Self::Curved { rulings, .. } => *rulings.last().expect("stations"),
        }
    }
}

fn rot90(v: P2) -> P2 {
    [-v[1], v[0]]
}

fn sub2(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn add2(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale2(a: P2, s: f64) -> P2 {
    [a[0] * s, a[1] * s]
}

fn cross2(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn len2(a: P2) -> f64 {
    a[0].hypot(a[1])
}

/// The point `dp` from `p` and `dq` from `q` on the side of the ruling
/// `ra → rb` the development runs to (its left), the nearer one to `p`
/// when the circles only touch.
fn placed(p: P2, q: P2, dp: f64, dq: f64, ra: P2, rb: P2) -> P2 {
    let d = sub2(q, p);
    let l = len2(d);
    if l <= 1e-300 {
        return p;
    }
    let e = scale2(d, 1.0 / l);
    let x = (dp * dp - dq * dq + l * l) / (2.0 * l);
    let y = ((dp - x) * (dp + x)).max(0.0).sqrt();
    let one = add2(p, add2(scale2(e, x), scale2(rot90(e), y)));
    let other = add2(p, add2(scale2(e, x), scale2(rot90(e), -y)));
    let r = sub2(rb, ra);
    if cross2(r, sub2(one, ra)) >= cross2(r, sub2(other, ra)) {
        one
    } else {
        other
    }
}

/// A curved band laid flat from its first ruling at (`a`, `b`) in `steps`
/// thin triangles: the ruling at every step.
fn strip_laid(
    t: &Transition,
    strip: usize,
    depth: f64,
    a: P2,
    b: P2,
    steps: usize,
) -> Vec<(P2, P2)> {
    let mut out = Vec::with_capacity(steps + 1);
    let (mut a2, mut b2) = (a, b);
    let (mut a3, mut b3) = t.curved_at(strip, 0.0, depth);
    out.push((a2, b2));
    for i in 1..=steps {
        let (na3, nb3) = t.curved_at(strip, i as f64 / steps as f64, depth);
        // A side that does not move (a cone's apex) stays where it is: the
        // circles' intersection would place it by the rounding of two
        // nearly equal lengths, a little to one side each step.
        let na2 = if distance(na3, a3) <= 1e-12 {
            a2
        } else {
            placed(a2, b2, distance(na3, a3), distance(na3, b3), a2, b2)
        };
        let nb2 = if distance(nb3, b3) <= 1e-12 {
            b2
        } else {
            placed(na2, b2, distance(nb3, na3), distance(nb3, b3), a2, b2)
        };
        a2 = na2;
        b2 = nb2;
        a3 = na3;
        b3 = nb3;
        out.push((a2, b2));
    }
    out
}

/// The surface `depth` behind the front laid flat, band by band, from its
/// first ruling along +y at the origin.
pub(super) fn developed(t: &Transition, depth: f64) -> Result<Vec<Developed>, RegionError> {
    let bands = t.bands();
    let common = depth;
    let mut out: Vec<Developed> = Vec::with_capacity(bands.len());
    let mut start: Option<(P2, P2)> = None;
    for (i, band) in bands.iter().enumerate() {
        let k = band.strip();
        // A bend whose inside is on the back at its own neutral depth.
        let depth = common + t.shift.get(k).copied().unwrap_or(0.0);
        let short = || RegionError::new("the profiles meet here", &t.strips[k]);
        match band {
            Band::Flat { normal, .. } => {
                let c = t.flat_at(&bands, i, depth);
                let length = distance(c[0], c[2]);
                if length <= NEAR {
                    return Err(short());
                }
                let (a2, b2) = start.unwrap_or(([0.0, 0.0], [0.0, length]));
                let er = scale(sub(c[2], c[0]), 1.0 / length);
                let eq = cross(*normal, er);
                let er2 = scale2(sub2(b2, a2), 1.0 / len2(sub2(b2, a2)).max(1e-300));
                let eq2 = rot90(er2);
                let flat = |p: V3| {
                    let d = sub(p, c[0]);
                    add2(a2, add2(scale2(er2, dot(d, er)), scale2(eq2, dot(d, eq))))
                };
                let corners = [a2, flat(c[1]), b2, flat(c[3])];
                start = Some((corners[1], corners[3]));
                out.push(Developed::Flat(corners));
            }
            Band::Curved { strip } => {
                let (a3, b3) = t.curved_at(*strip, 0.0, depth);
                let length = distance(a3, b3);
                if length <= NEAR {
                    return Err(short());
                }
                let (a2, b2) = start.unwrap_or(([0.0, 0.0], [0.0, length]));
                let coarse = strip_laid(t, *strip, depth, a2, b2, STEPS);
                let fine = strip_laid(t, *strip, depth, a2, b2, 2 * STEPS);
                // Richardson: the error falls with the square of the step.
                let at = |i: usize| -> (P2, P2) {
                    let (ca, cb) = coarse[i];
                    let (fa, fb) = fine[2 * i];
                    (
                        sub2(scale2(fa, 4.0 / 3.0), scale2(ca, 1.0 / 3.0)),
                        sub2(scale2(fb, 4.0 / 3.0), scale2(cb, 1.0 / 3.0)),
                    )
                };
                let every = STEPS / SPANS;
                let rulings: Vec<(P2, P2)> = (0..=SPANS).map(|k| at(k * every)).collect();
                // Derivatives by the fraction from the fine strip: central
                // differences, one-sided of the second order at the ends.
                let h = 1.0 / (2 * STEPS) as f64;
                let last = 2 * STEPS;
                let derivative = |i: usize, side: fn(&(P2, P2)) -> P2| -> P2 {
                    let p = |k: usize| side(&fine[k]);
                    if i == 0 {
                        scale2(
                            add2(
                                sub2(scale2(p(1), 4.0), scale2(p(0), 3.0)),
                                scale2(p(2), -1.0),
                            ),
                            1.0 / (2.0 * h),
                        )
                    } else if i == last {
                        scale2(
                            add2(
                                sub2(scale2(p(last), 3.0), scale2(p(last - 1), 4.0)),
                                p(last - 2),
                            ),
                            1.0 / (2.0 * h),
                        )
                    } else {
                        scale2(sub2(p(i + 1), p(i - 1)), 1.0 / (2.0 * h))
                    }
                };
                let derivatives = (0..=SPANS)
                    .map(|k| {
                        let i = 2 * k * every;
                        (derivative(i, |r| r.0), derivative(i, |r| r.1))
                    })
                    .collect();
                start = Some(*rulings.last().expect("stations"));
                out.push(Developed::Curved {
                    rulings,
                    derivatives,
                });
            }
        }
    }
    Ok(out)
}

/// Cubic Bézier spans through stations with their derivatives (by the
/// fraction over all spans), as B-spline poles: `3 n + 1` of them.
fn hermite_poles<T: Copy>(
    points: &[T],
    derivatives: &[T],
    add: fn(T, T) -> T,
    scale: fn(T, f64) -> T,
) -> Vec<T> {
    let n = points.len() - 1;
    let h = 1.0 / n as f64;
    let mut poles = Vec::with_capacity(3 * n + 1);
    for i in 0..n {
        poles.push(points[i]);
        poles.push(add(points[i], scale(derivatives[i], h / 3.0)));
        poles.push(add(points[i + 1], scale(derivatives[i + 1], -h / 3.0)));
    }
    poles.push(points[n]);
    poles
}

/// The knots of [`hermite_poles`]: 0 to n, the inner ones three times.
fn hermite_knots(n: usize) -> (Vec<f64>, Vec<u32>) {
    let knots = (0..=n).map(|i| i as f64).collect();
    let mut mults = vec![3; n + 1];
    mults[0] = 4;
    mults[n] = 4;
    (knots, mults)
}

/// A planar B-spline through stations, for a chart's curved edge.
pub(super) fn chart_spline(points: &[P2], derivatives: &[P2]) -> SegmentGeometry {
    let (knots, multiplicities) = hermite_knots(points.len() - 1);
    // Derivatives by the knot parameter: the fraction times the spans.
    SegmentGeometry::BSpline {
        degree: 3,
        poles: hermite_poles(points, derivatives, add2, scale2),
        weights: Vec::new(),
        knots,
        multiplicities,
        periodic: false,
    }
}

/// The developed outline: along the first profile, across the last
/// ruling, back along the second profile and across the first ruling.
pub(super) fn outline(developed: &[Developed]) -> Vec<SegmentGeometry> {
    let mut forward = Vec::new();
    let mut back = Vec::new();
    let line = |a: P2, b: P2| {
        (len2(sub2(b, a)) > 1e-9).then_some(SegmentGeometry::Line { start: a, end: b })
    };
    for d in developed {
        match d {
            Developed::Flat(c) => {
                forward.extend(line(c[0], c[1]));
                back.extend(line(c[3], c[2]));
            }
            Developed::Curved {
                rulings,
                derivatives,
            } => {
                let a: Vec<P2> = rulings.iter().map(|r| r.0).collect();
                let da: Vec<P2> = derivatives.iter().map(|r| r.0).collect();
                let b: Vec<P2> = rulings.iter().rev().map(|r| r.1).collect();
                let db: Vec<P2> = derivatives
                    .iter()
                    .rev()
                    .map(|r| scale2(r.1, -1.0))
                    .collect();
                if len2(sub2(a[a.len() - 1], a[0])) > 1e-9 {
                    forward.push(chart_spline(&a, &da));
                }
                if len2(sub2(b[b.len() - 1], b[0])) > 1e-9 {
                    back.push(chart_spline(&b, &db));
                }
            }
        }
    }
    let (first, last) = (
        developed.first().expect("bands").start(),
        developed.last().expect("bands").end(),
    );
    let mut out = forward;
    out.extend(line(last.0, last.1));
    back.reverse();
    out.extend(back);
    out.extend(line(first.1, first.0));
    out
}

/// The outline's points (lines' ends, curves sampled), for overlap checks.
pub(super) fn outline_points(developed: &[Developed]) -> Vec<P2> {
    let mut forward = Vec::new();
    let mut back = Vec::new();
    for d in developed {
        match d {
            Developed::Flat(c) => {
                forward.push(c[0]);
                back.push(c[2]);
            }
            Developed::Curved { rulings, .. } => {
                for r in &rulings[..rulings.len() - 1] {
                    forward.push(r.0);
                    back.push(r.1);
                }
            }
        }
    }
    let last = developed.last().expect("bands").end();
    forward.push(last.0);
    back.push(last.1);
    back.reverse();
    forward.extend(back);
    forward.dedup_by(|a, b| len2(sub2(*a, *b)) <= 1e-9);
    forward
}

fn segments_cross(p1: P2, p2: P2, q1: P2, q2: P2) -> bool {
    let d1 = cross2(sub2(p2, p1), sub2(q1, p1));
    let d2 = cross2(sub2(p2, p1), sub2(q2, p1));
    let d3 = cross2(sub2(q2, q1), sub2(p1, q1));
    let d4 = cross2(sub2(q2, q1), sub2(p2, q1));
    let tol = 1e-9;
    ((d1 > tol && d2 < -tol) || (d1 < -tol && d2 > tol))
        && ((d3 > tol && d4 < -tol) || (d3 < -tol && d4 > tol))
}

/// Whether the outline crosses itself (the blank would overlap).
pub(super) fn overlaps(points: &[P2]) -> bool {
    let n = points.len();
    for i in 0..n {
        let (p1, p2) = (points[i], points[(i + 1) % n]);
        for j in i + 2..n {
            if i == 0 && j + 1 == n {
                continue;
            }
            let (q1, q2) = (points[j], points[(j + 1) % n]);
            if segments_cross(p1, p2, q1, q2) {
                return true;
            }
        }
    }
    false
}

/// The rulings of the surface `depth` behind the front in order (every
/// band's start, a curved band's stations, the last band's end): their
/// ends in the body and in the chart, the material direction there and how
/// far behind the front they lie (`depth`, or a bend's own neutral depth,
/// mitcad#2560).
pub(super) fn neutral_rulings(
    t: &Transition,
    developed: &[Developed],
    depth: f64,
) -> Vec<([V3; 2], [P2; 2], V3, f64)> {
    let bands = t.bands();
    let at = |strip: usize| depth + t.shift.get(strip).copied().unwrap_or(0.0);
    let mut out = Vec::new();
    for (i, band) in bands.iter().enumerate() {
        match (band, &developed[i]) {
            (Band::Flat { normal, strip, .. }, Developed::Flat(c2)) => {
                let d = at(*strip);
                let c = t.flat_at(&bands, i, d);
                out.push(([c[0], c[2]], [c2[0], c2[2]], scale(*normal, t.side), d));
            }
            (Band::Curved { strip }, Developed::Curved { rulings, .. }) => {
                let d = at(*strip);
                for (k, r) in rulings.iter().enumerate().take(rulings.len() - 1) {
                    let f = k as f64 / SPANS as f64;
                    let (a, b) = t.curved_at(*strip, f, d);
                    let n = t.strips[*strip].normal(f).expect("checked");
                    out.push(([a, b], [r.0, r.1], scale(n, t.side), d));
                }
            }
            _ => {}
        }
    }
    if let (Some(band), Some(dev)) = (bands.last(), developed.last()) {
        let (a2, b2) = dev.end();
        let i = bands.len() - 1;
        let d = at(band.strip());
        let (model, n) = match *band {
            Band::Flat { normal, .. } => {
                let c = t.flat_at(&bands, i, d);
                ([c[1], c[3]], normal)
            }
            Band::Curved { strip } => {
                let (a, b) = t.curved_at(strip, 1.0, d);
                ([a, b], t.strips[strip].normal(1.0).expect("checked"))
            }
        };
        out.push((model, [a2, b2], scale(n, t.side), d));
    }
    out
}

/// A bend's lines in the chart (mitcad#2560): its centre line (the middle
/// ruling of its band) and the lines where it meets the facets beside it.
pub(super) fn bend_lines(
    t: &Transition,
    developed: &[Developed],
    strip: usize,
) -> Option<([P2; 2], [[P2; 2]; 2])> {
    let i = t
        .bands()
        .iter()
        .position(|b| matches!(b, Band::Curved { strip: k } if *k == strip))?;
    let Developed::Curved { rulings, .. } = developed.get(i)? else {
        return None;
    };
    let line = |r: (P2, P2)| [r.0, r.1];
    Some((
        line(rulings[SPANS / 2]),
        [line(rulings[0]), line(rulings[SPANS])],
    ))
}

/// The developed rulings between strips: the last ruling of each strip's
/// bands (at the mitre where it folds), for strips 0 to n - 2.
pub(super) fn boundary_rulings(t: &Transition, developed: &[Developed]) -> Vec<(P2, P2)> {
    let bands = t.bands();
    let mut out = Vec::new();
    for k in 0..t.strips.len().saturating_sub(1) {
        let last = bands
            .iter()
            .rposition(|b| b.strip() == k)
            .expect("every strip has a band");
        out.push(developed[last].end());
    }
    out
}

fn bilinear(face: FaceName, poles: [V3; 4]) -> SurfacePatch {
    SurfacePatch {
        face,
        u_degree: 1,
        v_degree: 1,
        nu: 2,
        nv: 2,
        poles: poles.to_vec(),
        u_knots: vec![0.0, 1.0],
        u_mults: vec![2, 2],
        v_knots: vec![0.0, 1.0],
        v_mults: vec![2, 2],
    }
}

/// A patch ruled between two curves of the same spans (cubic along them).
fn ruled(face: FaceName, first: &[V3], second: &[V3]) -> SurfacePatch {
    let spans = (first.len() - 1) / 3;
    let (knots, mults) = hermite_knots(spans);
    let mut poles = Vec::with_capacity(first.len() * 2);
    for (p, q) in first.iter().zip(second) {
        poles.push(*p);
        poles.push(*q);
    }
    SurfacePatch {
        face,
        u_degree: 3,
        v_degree: 1,
        nu: first.len(),
        nv: 2,
        poles,
        u_knots: knots,
        u_mults: mults,
        v_knots: vec![0.0, 1.0],
        v_mults: vec![2, 2],
    }
}

/// Whether a set of points has no extent.
fn collapsed(points: &[V3]) -> bool {
    points.iter().all(|p| near(*p, points[0]))
}

/// The patches of the folded body: the front and the back of every band,
/// the walls along both profiles and across the first and last rulings.
/// Faces `<feature>:lofted_front(i)`, `lofted_back(i)`, `lofted_first(i)`
/// (along the first profile), `lofted_second(i)`, `lofted_start(0)` and
/// `lofted_end(0)`.
pub(super) fn patches(t: &Transition, thickness: f64, feature: FeatureUid) -> Vec<SurfacePatch> {
    let bands = t.bands();
    let name = |role: &str, i: usize| FaceName::new(feature, role, Some(RoleKey::Index(i as u32)));
    let mut out = Vec::new();
    // Each band's sides at the front and at the back: B-spline poles along
    // the first and the second profile.
    let sides = |i: usize, depth: f64| -> (Vec<V3>, Vec<V3>) {
        match bands[i] {
            Band::Flat { .. } => {
                let c = t.flat_at(&bands, i, depth);
                (vec![c[0], c[1]], vec![c[2], c[3]])
            }
            Band::Curved { strip } => {
                let stations: Vec<f64> = (0..=SPANS).map(|k| k as f64 / SPANS as f64).collect();
                let points: Vec<(V3, V3)> = stations
                    .iter()
                    .map(|f| t.curved_at(strip, *f, depth))
                    .collect();
                let derivatives: Vec<(V3, V3)> = stations
                    .iter()
                    .map(|f| t.curved_derivative(strip, *f, depth))
                    .collect();
                let pa: Vec<V3> = points.iter().map(|p| p.0).collect();
                let pb: Vec<V3> = points.iter().map(|p| p.1).collect();
                let da: Vec<V3> = derivatives.iter().map(|p| p.0).collect();
                let db: Vec<V3> = derivatives.iter().map(|p| p.1).collect();
                (
                    hermite_poles(&pa, &da, add, scale),
                    hermite_poles(&pb, &db, add, scale),
                )
            }
        }
    };
    for (i, band) in bands.iter().enumerate() {
        let (fa, fb) = sides(i, 0.0);
        let (ba, bb) = sides(i, thickness);
        let flat = matches!(band, Band::Flat { .. });
        for (role, a, b) in [("lofted_front", &fa, &fb), ("lofted_back", &ba, &bb)] {
            if collapsed(a) && collapsed(b) {
                continue;
            }
            out.push(if flat {
                bilinear(name(role, i), [a[0], b[0], a[1], b[1]])
            } else {
                ruled(name(role, i), a, b)
            });
        }
        for (role, front, back) in [("lofted_first", &fa, &ba), ("lofted_second", &fb, &bb)] {
            if collapsed(front) && collapsed(back) {
                continue;
            }
            out.push(if flat {
                bilinear(name(role, i), [front[0], back[0], front[1], back[1]])
            } else {
                ruled(name(role, i), front, back)
            });
        }
    }
    let ends = |i: usize, first: bool| -> [V3; 4] {
        let (fa, fb) = sides(i, 0.0);
        let (ba, bb) = sides(i, thickness);
        let pick = |v: &Vec<V3>| if first { v[0] } else { v[v.len() - 1] };
        [pick(&fa), pick(&ba), pick(&fb), pick(&bb)]
    };
    out.push(bilinear(name("lofted_start", 0), ends(0, true)));
    out.push(bilinear(
        name("lofted_end", 0),
        ends(bands.len() - 1, false),
    ));
    out
}

/// A run of a profile: its piece from fraction `from` to `to` of it.
#[derive(Debug, Clone)]
pub(super) struct Run {
    pub piece: Piece,
    pub from: f64,
    pub to: f64,
}

impl Run {
    pub fn point(&self, f: f64) -> V3 {
        self.piece.point(self.from + (self.to - self.from) * f)
    }

    /// The derivative by the fraction `f`.
    pub fn derivative(&self, f: f64) -> V3 {
        scale(
            self.piece.derivative(self.from + (self.to - self.from) * f),
            self.to - self.from,
        )
    }

    fn part(&self, f0: f64, f1: f64) -> Self {
        let d = self.to - self.from;
        Self {
            piece: self.piece.clone(),
            from: self.from + d * f0,
            to: self.from + d * f1,
        }
    }
}

/// A band of a ruled transition (a die formed one laid flat by
/// triangulation, mitcad#2560): between a run of the first profile and one
/// of the second, its lines at the same fraction `f` of both.
#[derive(Debug, Clone)]
pub(super) struct RuledBand {
    pub a: Run,
    pub b: Run,
}

impl RuledBand {
    /// The point at `f` along it, `v` across from the first profile.
    pub fn point(&self, f: f64, v: f64) -> V3 {
        lerp3(self.a.point(f), self.b.point(f), v)
    }

    /// The unit normal there, as a strip's: the line across times the way
    /// it runs.
    pub fn normal(&self, f: f64, v: f64) -> Option<V3> {
        let along = add(
            scale(self.a.derivative(f), 1.0 - v),
            scale(self.b.derivative(f), v),
        );
        unit3(cross(sub(self.b.point(f), self.a.point(f)), along))
    }
}

/// A ruled band's grid for a triangulated development (mitcad#2560):
/// `along` steps along it, `across` from the first profile to the second,
/// the normals out of the body (against the material, `side` times the
/// band's normals). None where the band has no normal.
pub(super) fn ruled_grid(
    band: &RuledBand,
    along: usize,
    across: usize,
    side: f64,
) -> Option<crate::kernel::SheetFaceGrid> {
    let mut points = Vec::with_capacity((along + 1) * (across + 1));
    let mut normals = Vec::with_capacity(points.capacity());
    for j in 0..=across {
        for i in 0..=along {
            let (f, v) = (i as f64 / along as f64, j as f64 / across as f64);
            points.push(band.point(f, v));
            normals.push(scale(band.normal(f, v)?, -side));
        }
    }
    let mut grid = crate::kernel::SheetFaceGrid {
        columns: along,
        rows: across,
        points,
        normals,
        area: 0.0,
        wires: Vec::new(),
    };
    grid.area = super::super::triangulated::grid_area(&grid, 0.0);
    Some(grid)
}

/// The pieces' lengths and where each starts along the chain.
fn starts(chain: &Chain) -> (Vec<f64>, Vec<f64>) {
    let lengths: Vec<f64> = chain.pieces.iter().map(Piece::length).collect();
    let mut at = Vec::with_capacity(lengths.len());
    let mut walked = 0.0;
    for l in &lengths {
        at.push(walked);
        walked += l;
    }
    (lengths, at)
}

/// The point `u` along a chain from its start (round again past the end of
/// a closed one).
pub(super) fn point_along(chain: &Chain, u: f64) -> V3 {
    let (lengths, at) = starts(chain);
    let total: f64 = lengths.iter().sum();
    let u = if chain.closed && total > 0.0 {
        u.rem_euclid(total)
    } else {
        u.clamp(0.0, total)
    };
    let i = at.iter().rposition(|s| *s <= u).unwrap_or(0);
    let f = if lengths[i] > 0.0 {
        ((u - at[i]) / lengths[i]).clamp(0.0, 1.0)
    } else {
        0.0
    };
    chain.pieces[i].point(f)
}

/// The chain from `start` along it for `length` (round a closed one) as
/// runs, each with where it starts and ends from `start`.
fn runs(chain: &Chain, start: f64, length: f64) -> Vec<(Run, f64, f64)> {
    let (lengths, at) = starts(chain);
    let total: f64 = lengths.iter().sum();
    let mut out = Vec::new();
    let mut g = if chain.closed {
        start.rem_euclid(total)
    } else {
        start
    };
    let mut walked = 0.0;
    let mut guard = 0;
    while length - walked > 1e-9 && guard < 4 * lengths.len() + 4 {
        guard += 1;
        let i = at
            .iter()
            .rposition(|s| *s <= g + 1e-12)
            .unwrap_or(0)
            .min(lengths.len() - 1);
        let end = (at[i] + lengths[i]).min(g + (length - walked));
        if end - g > 1e-9 && lengths[i] > 0.0 {
            out.push((
                Run {
                    piece: chain.pieces[i].clone(),
                    from: (g - at[i]) / lengths[i],
                    to: (end - at[i]) / lengths[i],
                },
                walked,
                walked + (end - g),
            ));
        }
        walked += (end - g).max(0.0);
        g = end;
        if chain.closed && g >= total - 1e-12 {
            g = 0.0;
        } else if g >= at[i] + lengths[i] - 1e-12 && i + 1 < lengths.len() {
            g = at[i + 1];
        }
    }
    out
}

/// Where along `chain` a point lies nearest: its length from the start
/// (sampled, then refined between the samples beside the best).
fn nearest_along(chain: &Chain, p: V3) -> f64 {
    let total = chain.length();
    let n = 1024;
    let d = |u: f64| distance(point_along(chain, u), p);
    let mut best = (0.0, f64::INFINITY);
    for i in 0..=n {
        let u = total * i as f64 / n as f64;
        let l = d(u);
        if l < best.1 {
            best = (u, l);
        }
    }
    let step = total / n as f64;
    let (mut lo, mut hi) = (best.0 - step, best.0 + step);
    for _ in 0..60 {
        let (m1, m2) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
        if d(m1) <= d(m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    (lo + hi) / 2.0
}

/// The ruled transition between two profiles (a die formed one laid flat
/// by triangulation, mitcad#2560): its lines join the points at the same
/// share of both profiles' lengths, from the seam (a closed transition's:
/// at the `seam` share of the first profile and the nearest point of the
/// second, or the shortest such line) less `gap / 2` each side of it; a
/// band between every two places where either profile has a corner or
/// another curve begins, so each band is smooth.
pub(super) fn ruled_transition(
    first: &Chain,
    second: &Chain,
    seam: Option<f64>,
    gap: f64,
) -> Result<Vec<RuledBand>, String> {
    let (la, lb) = (first.length(), second.length());
    let (ra, rb) = if first.closed {
        let (sa, sb) = match seam {
            Some(f) => {
                let sa = f * la;
                (sa, nearest_along(second, point_along(first, sa)))
            }
            None => {
                let n = 256;
                let mut best = (0.0, 0.0, f64::INFINITY);
                for i in 0..n {
                    let sa = la * i as f64 / n as f64;
                    let p = point_along(first, sa);
                    let sb = nearest_along(second, p);
                    let l = distance(p, point_along(second, sb));
                    if l < best.2 - 1e-9 {
                        best = (sa, sb, l);
                    }
                }
                (best.0, best.1)
            }
        };
        if gap >= la.min(lb) {
            return Err("the seam's gap is longer than a profile".into());
        }
        let half = gap / 2.0;
        (
            runs(first, sa + half, la - gap),
            runs(second, sb + half, lb - gap),
        )
    } else {
        (runs(first, 0.0, la), runs(second, 0.0, lb))
    };
    let (ta, tb) = (
        ra.last().map_or(0.0, |r| r.2),
        rb.last().map_or(0.0, |r| r.2),
    );
    if ta <= NEAR || tb <= NEAR {
        return Err("a profile has no length".into());
    }
    // Where either profile's runs change, as shares of its length.
    let mut breaks: Vec<f64> = ra
        .iter()
        .map(|r| r.2 / ta)
        .chain(rb.iter().map(|r| r.2 / tb))
        .collect();
    breaks.push(0.0);
    breaks.sort_by(f64::total_cmp);
    breaks.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
    if let Some(last) = breaks.last_mut() {
        *last = 1.0;
    }
    let part = |runs: &[(Run, f64, f64)], total: f64, f0: f64, f1: f64| -> Option<Run> {
        let mid = (f0 + f1) / 2.0 * total;
        let (run, s, e) = runs
            .iter()
            .find(|r| r.1 <= mid + 1e-12 && mid <= r.2 + 1e-12)?;
        let w = e - s;
        Some(run.part(
            ((f0 * total - s) / w).clamp(0.0, 1.0),
            ((f1 * total - s) / w).clamp(0.0, 1.0),
        ))
    };
    let mut out = Vec::new();
    for w in breaks.windows(2) {
        let (f0, f1) = (w[0], w[1]);
        if f1 - f0 <= 1e-9 {
            continue;
        }
        let (Some(a), Some(b)) = (part(&ra, ta, f0, f1), part(&rb, tb, f0, f1)) else {
            return Err("a profile's curves could not be followed along it".into());
        };
        out.push(RuledBand { a, b });
    }
    if out.is_empty() {
        return Err("the profiles have no length".into());
    }
    Ok(out)
}

/// The poles of a cubic through stations with their derivatives (by the
/// fraction from the first station to the last) whose inner knots are
/// twice, of [`smooth_knots`]: the curve of [`hermite_poles`], continuous in
/// its first derivative in the knots as well as in its shape. Each
/// station's own pole is left out, the midpoint of the poles beside it.
fn smooth_poles(points: &[V3], derivatives: &[V3]) -> Vec<V3> {
    let n = points.len() - 1;
    let h = 1.0 / n as f64;
    let mut poles = Vec::with_capacity(2 * n + 2);
    poles.push(points[0]);
    for i in 0..=n {
        if i > 0 {
            poles.push(add(points[i], scale(derivatives[i], -h / 3.0)));
        }
        if i < n {
            poles.push(add(points[i], scale(derivatives[i], h / 3.0)));
        }
    }
    poles.push(points[n]);
    poles
}

/// The knots of [`smooth_poles`]: 0 to n, the inner ones twice.
fn smooth_knots(n: usize) -> (Vec<f64>, Vec<u32>) {
    let knots = (0..=n).map(|i| i as f64).collect();
    let mut mults = vec![2; n + 1];
    mults[0] = 4;
    mults[n] = 4;
    (knots, mults)
}

/// The front surface of a ruled transition as patches: a band between two
/// straight runs bilinear, others cubic along the profiles through
/// `SPANS + 1` exact stations with their tangents and straight across.
/// Faces `<feature>:lofted_front(i)`. The cubic's inner knots are twice
/// ([`smooth_knots`]): the surface is continuous in its first derivative,
/// as the kernel's offset that makes it solid needs (it refuses a surface
/// whose knots repeat as often as its degree, which is continuous only in
/// its points by its knots, mitcad#2560).
pub(super) fn ruled_patches(bands: &[RuledBand], feature: FeatureUid) -> Vec<SurfacePatch> {
    let name = |i: usize| FaceName::new(feature, "lofted_front", Some(RoleKey::Index(i as u32)));
    bands
        .iter()
        .enumerate()
        .map(|(i, band)| {
            if matches!(band.a.piece, Piece::Line(..)) && matches!(band.b.piece, Piece::Line(..)) {
                return bilinear(
                    name(i),
                    [
                        band.a.point(0.0),
                        band.b.point(0.0),
                        band.a.point(1.0),
                        band.b.point(1.0),
                    ],
                );
            }
            let f = |k: usize| k as f64 / SPANS as f64;
            let pa: Vec<V3> = (0..=SPANS).map(|k| band.a.point(f(k))).collect();
            let pb: Vec<V3> = (0..=SPANS).map(|k| band.b.point(f(k))).collect();
            let da: Vec<V3> = (0..=SPANS).map(|k| band.a.derivative(f(k))).collect();
            let db: Vec<V3> = (0..=SPANS).map(|k| band.b.derivative(f(k))).collect();
            let (first, second) = (smooth_poles(&pa, &da), smooth_poles(&pb, &db));
            let (u_knots, u_mults) = smooth_knots(SPANS);
            SurfacePatch {
                face: name(i),
                u_degree: 3,
                v_degree: 1,
                nu: first.len(),
                nv: 2,
                poles: first
                    .iter()
                    .zip(&second)
                    .flat_map(|(p, q)| [*p, *q])
                    .collect(),
                u_knots,
                u_mults,
                v_knots: vec![0.0, 1.0],
                v_mults: vec![2, 2],
            }
        })
        .collect()
}

/// The steps of a ruled transition's back across it, from the first
/// profile to the second (mitcad#2560).
const BACK_SPANS: usize = 16;
/// The step of the differences that give a ruled transition's back its
/// derivatives (fractions of a band).
const BACK_STEP: f64 = 1e-4;
/// The steps a crease's line on the back is followed in, from the first
/// profile to the second.
const CREASE_STEPS: usize = 512;
/// The sine of the angle between two bands' normals below which they meet
/// smoothly there.
const SMOOTH: f64 = 1e-7;

const NO_NORMAL: &str = "the profiles meet here: a lofted flange needs room between them";
const TOO_THICK: &str =
    "the sheet is too thick for the folds of its transition, or a fold turns back on itself";

/// Where two bands' backs meet at the crease between them: at each place
/// `w` along the crease (0 at the first profile, 1 at the second), the
/// fractions along and across of the earlier band's back and of the later
/// one's.
enum Crease {
    /// The bands meet smoothly all along: their backs meet at their ends.
    Smooth,
    /// Their backs' line of intersection, followed in `CREASE_STEPS` steps.
    Line(Vec<[f64; 4]>),
}

/// The back of a ruled transition made solid (mitcad#2560): each band's
/// front moved `depth` along its normals (its offset, exact), and where
/// two bands meet at an angle (a crease along the ruling from a corner),
/// both backs stretched where they part, or cut back where they cross, to
/// the line where they meet (as an offset with sharp joins makes it),
/// found in the plane square to the crease at each place along it; a
/// crease that fades out to a smooth join meets there at the bands' own
/// ends. Built by the transition itself: the kernel's offset cannot offset
/// faces along a crease that fades out to a smooth join (as the creases
/// from one profile's corners to a smooth curve of the other do).
pub(super) struct RuledBack<'a> {
    bands: &'a [RuledBand],
    depth: f64,
    creases: Vec<Crease>,
}

impl<'a> RuledBack<'a> {
    /// `depth`: the thickness, negative against the bands' normals.
    pub fn new(bands: &'a [RuledBand], depth: f64) -> Result<Self, String> {
        let mut back = Self {
            bands,
            depth,
            creases: Vec::new(),
        };
        for k in 0..bands.len().saturating_sub(1) {
            let crease = back.follow(k)?;
            back.creases.push(crease);
        }
        Ok(back)
    }

    /// Band `i`'s front moved along its normal, at `f` along it (past its
    /// ends too) and `v` across.
    fn offset(&self, i: usize, f: f64, v: f64) -> Result<V3, String> {
        let band = &self.bands[i];
        let n = band.normal(f, v).ok_or(NO_NORMAL)?;
        Ok(add(band.point(f, v), scale(n, self.depth)))
    }

    /// Whether bands `k` and `k + 1` meet smoothly `v` across.
    fn smooth(&self, k: usize, v: f64) -> Result<bool, String> {
        let a = self.bands[k].normal(1.0, v).ok_or(NO_NORMAL)?;
        let b = self.bands[k + 1].normal(0.0, v).ok_or(NO_NORMAL)?;
        Ok(dot(a, b) > 0.0 && norm(cross(a, b)) <= SMOOTH)
    }

    /// The crease between bands `k` and `k + 1` followed along, each
    /// place from the one before.
    fn follow(&self, k: usize) -> Result<Crease, String> {
        let (start, end) = (self.smooth(k, 0.0)?, self.smooth(k, 1.0)?);
        if start && end && self.smooth(k, 0.5)? {
            return Ok(Crease::Smooth);
        }
        let mut line: Vec<[f64; 4]> = Vec::with_capacity(CREASE_STEPS + 1);
        for m in 0..=CREASE_STEPS {
            let w = m as f64 / CREASE_STEPS as f64;
            let x = if (m == 0 && start) || (m == CREASE_STEPS && end) {
                // A smooth end: the backs meet at the bands' ends.
                [1.0, w, 0.0, w]
            } else {
                let guess = match line.len() {
                    0 => [1.0, w, 0.0, w],
                    1 => line[0],
                    n => {
                        let (a, b) = (line[n - 2], line[n - 1]);
                        [0, 1, 2, 3].map(|c| 2.0 * b[c] - a[c])
                    }
                };
                self.meet(k, w, guess)?
            };
            line.push(x);
        }
        Ok(Crease::Line(line))
    }

    /// Where the backs of bands `k` and `k + 1` meet in the plane square to
    /// the crease at `w` along it, by Newton's method from `guess`: the
    /// fractions along and across of both.
    fn meet(&self, k: usize, w: f64, guess: [f64; 4]) -> Result<[f64; 4], String> {
        let band = &self.bands[k];
        let at = band.point(1.0, w);
        let along = unit3(sub(band.b.point(1.0), band.a.point(1.0))).ok_or(NO_NORMAL)?;
        let residual = |x: [f64; 4]| -> Result<[f64; 4], String> {
            let p = self.offset(k, x[0], x[1])?;
            let q = self.offset(k + 1, x[2], x[3])?;
            let d = sub(p, q);
            Ok([d[0], d[1], d[2], dot(sub(p, at), along)])
        };
        let size = |r: [f64; 4]| r.iter().map(|c| c * c).sum::<f64>().sqrt();
        let mut x = guess;
        for _ in 0..60 {
            let r = residual(x)?;
            if size(r) <= 1e-13 {
                return Ok(x);
            }
            let h = 1e-7;
            let mut jacobian = [[0.0; 4]; 4];
            for c in 0..4 {
                let (mut plus, mut minus) = (x, x);
                plus[c] += h;
                minus[c] -= h;
                let (rp, rm) = (residual(plus)?, residual(minus)?);
                for (row, (p, m)) in jacobian.iter_mut().zip(rp.iter().zip(&rm)) {
                    row[c] = (p - m) / (2.0 * h);
                }
            }
            let step = solve4(jacobian, r).ok_or(TOO_THICK)?;
            for (xc, sc) in x.iter_mut().zip(&step) {
                *xc -= sc;
            }
            if x.iter().any(|c| !(-0.5..=1.5).contains(c)) {
                return Err(TOO_THICK.into());
            }
            // Down to the rounding of the points: the step itself.
            if size(step) <= 1e-15 {
                break;
            }
        }
        if size(residual(x)?) <= 1e-8 {
            Ok(x)
        } else {
            Err(TOO_THICK.into())
        }
    }

    /// Where crease `k`'s backs meet at `w` along it.
    fn crease_at(&self, k: usize, w: f64) -> Result<[f64; 4], String> {
        match &self.creases[k] {
            Crease::Smooth => Ok([1.0, w, 0.0, w]),
            Crease::Line(line) => {
                let at = w.clamp(0.0, 1.0) * CREASE_STEPS as f64;
                let m = (at.floor() as usize).min(CREASE_STEPS - 1);
                let t = at - m as f64;
                if t == 0.0 {
                    return Ok(line[m]);
                }
                if t == 1.0 {
                    return Ok(line[m + 1]);
                }
                let guess = [0, 1, 2, 3].map(|c| line[m][c] + (line[m + 1][c] - line[m][c]) * t);
                self.meet(k, w, guess)
            }
        }
    }

    /// Band `i`'s back's ends at `w` along it: the fractions along and
    /// across of the band where it begins and where it ends.
    pub fn ends(&self, i: usize, w: f64) -> Result<([f64; 2], [f64; 2]), String> {
        let lo = if i == 0 {
            [0.0, w]
        } else {
            let x = self.crease_at(i - 1, w)?;
            [x[2], x[3]]
        };
        let hi = if i + 1 == self.bands.len() {
            [1.0, w]
        } else {
            let x = self.crease_at(i, w)?;
            [x[0], x[1]]
        };
        if hi[0] - lo[0] <= 1e-6 {
            return Err(TOO_THICK.into());
        }
        Ok((lo, hi))
    }

    /// Band `i`'s back at `s` (0 to 1) between its ends at `w` along the
    /// creases (0 at the first profile, 1 at the second): the point, and
    /// where on the band's front it stands. For checks: the body's patches
    /// sample the backs through [`BackGrid`].
    #[cfg(test)]
    pub fn place(&self, i: usize, s: f64, w: f64) -> Result<(V3, [f64; 2]), String> {
        let (lo, hi) = self.ends(i, w)?;
        let (f, v) = (lo[0] + (hi[0] - lo[0]) * s, lo[1] + (hi[1] - lo[1]) * s);
        Ok((self.offset(i, f, v)?, [f, v]))
    }

    /// Band `i`'s back at `s` between its ends and `w` along the creases
    /// (for checks).
    #[cfg(test)]
    pub fn point(&self, i: usize, s: f64, w: f64) -> Result<V3, String> {
        Ok(self.place(i, s, w)?.0)
    }
}

/// The solution of `a x = b` (4 by 4, partial pivoting); none when `a` is
/// singular.
fn solve4(mut a: [[f64; 4]; 4], mut b: [f64; 4]) -> Option<[f64; 4]> {
    for c in 0..4 {
        let pivot = (c..4).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[pivot][c].abs() <= 1e-300 {
            return None;
        }
        a.swap(c, pivot);
        b.swap(c, pivot);
        for r in c + 1..4 {
            let factor = a[r][c] / a[c][c];
            let above = a[c];
            for (x, y) in a[r].iter_mut().zip(&above).skip(c) {
                *x -= factor * y;
            }
            b[r] -= factor * b[c];
        }
    }
    let mut x = [0.0; 4];
    for c in (0..4).rev() {
        let known: f64 = (c + 1..4).map(|k| a[c][k] * x[k]).sum();
        x[c] = (b[c] - known) / a[c][c];
    }
    Some(x)
}

/// A band's back sampled at the stations of its patch: points and their
/// derivatives by `s` (along, 0 to 1) and `w` (along the creases, across
/// the band), and the twist.
struct BackGrid {
    points: Vec<Vec<V3>>,
    ds: Vec<Vec<V3>>,
    dv: Vec<Vec<V3>>,
    dsv: Vec<Vec<V3>>,
}

impl BackGrid {
    fn new(back: &RuledBack<'_>, i: usize) -> Result<Self, String> {
        let h = BACK_STEP;
        let mut grid = Self {
            points: Vec::new(),
            ds: Vec::new(),
            dv: Vec::new(),
            dsv: Vec::new(),
        };
        for l in 0..=BACK_SPANS {
            let w = l as f64 / BACK_SPANS as f64;
            // The derivative across from three places a step apart: about
            // w inside, from it into the band at the profiles (the backs'
            // ends are followed between them only). `here` is w's place.
            let (offsets, weights, here): ([f64; 3], [f64; 3], usize) = if l == 0 {
                ([0.0, h, 2.0 * h], [-1.5 / h, 2.0 / h, -0.5 / h], 0)
            } else if l == BACK_SPANS {
                ([-2.0 * h, -h, 0.0], [0.5 / h, -2.0 / h, 1.5 / h], 2)
            } else {
                ([-h, 0.0, h], [-0.5 / h, 0.0, 0.5 / h], 1)
            };
            let ends = [
                back.ends(i, w + offsets[0])?,
                back.ends(i, w + offsets[1])?,
                back.ends(i, w + offsets[2])?,
            ];
            let at = |s: f64, k: usize| -> Result<V3, String> {
                let (lo, hi) = ends[k];
                back.offset(i, lo[0] + (hi[0] - lo[0]) * s, lo[1] + (hi[1] - lo[1]) * s)
            };
            let across = |s: f64| -> Result<V3, String> {
                let mut sum = [0.0; 3];
                for (k, weight) in weights.iter().enumerate() {
                    sum = add(sum, scale(at(s, k)?, *weight));
                }
                Ok(sum)
            };
            let (mut points, mut ds, mut dv, mut dsv) =
                (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for k in 0..=SPANS {
                let s = k as f64 / SPANS as f64;
                points.push(at(s, here)?);
                ds.push(scale(sub(at(s + h, here)?, at(s - h, here)?), 0.5 / h));
                dv.push(across(s)?);
                dsv.push(scale(sub(across(s + h)?, across(s - h)?), 0.5 / h));
            }
            grid.points.push(points);
            grid.ds.push(ds);
            grid.dv.push(dv);
            grid.dsv.push(dsv);
        }
        Ok(grid)
    }

    /// The back across at station `k` along: its points and their
    /// derivatives across.
    fn column(&self, k: usize) -> (Vec<V3>, Vec<V3>) {
        (
            self.points.iter().map(|row| row[k]).collect(),
            self.dv.iter().map(|row| row[k]).collect(),
        )
    }

    /// The patch: bicubic through the stations with their derivatives
    /// and twists, its inner knots twice both ways.
    fn patch(&self, face: FaceName) -> SurfacePatch {
        // A pole's station and side: before it (-1), on it (0, at the
        // ends) or after it (1), as smooth_poles orders them.
        let entries = |n: usize| -> Vec<(usize, f64)> {
            let mut out = vec![(0, 0.0)];
            for i in 0..=n {
                if i > 0 {
                    out.push((i, -1.0));
                }
                if i < n {
                    out.push((i, 1.0));
                }
            }
            out.push((n, 0.0));
            out
        };
        let (hs, hv) = (1.0 / SPANS as f64, 1.0 / BACK_SPANS as f64);
        let (along, across) = (entries(SPANS), entries(BACK_SPANS));
        let mut poles = Vec::with_capacity(along.len() * across.len());
        for &(k, su) in &along {
            for &(l, sv) in &across {
                let mut p = self.points[l][k];
                p = add(p, scale(self.ds[l][k], su * hs / 3.0));
                p = add(p, scale(self.dv[l][k], sv * hv / 3.0));
                p = add(p, scale(self.dsv[l][k], su * sv * hs * hv / 9.0));
                poles.push(p);
            }
        }
        let (u_knots, u_mults) = smooth_knots(SPANS);
        let (v_knots, v_mults) = smooth_knots(BACK_SPANS);
        SurfacePatch {
            face,
            u_degree: 3,
            v_degree: 3,
            nu: along.len(),
            nv: across.len(),
            poles,
            u_knots,
            u_mults,
            v_knots,
            v_mults,
        }
    }
}

/// A patch ruled between two curves of [`smooth_poles`] of the same spans.
fn smooth_ruled(face: FaceName, first: &[V3], second: &[V3]) -> SurfacePatch {
    let (knots, mults) = smooth_knots((first.len() - 2) / 2);
    SurfacePatch {
        face,
        u_degree: 3,
        v_degree: 1,
        nu: first.len(),
        nv: 2,
        poles: first
            .iter()
            .zip(second)
            .flat_map(|(p, q)| [*p, *q])
            .collect(),
        u_knots: knots,
        u_mults: mults,
        v_knots: vec![0.0, 1.0],
        v_mults: vec![2, 2],
    }
}

/// The patches of a ruled transition made solid `depth` along its bands'
/// normals (negative: against them), mitcad#2560: the fronts of
/// [`ruled_patches`], the backs of [`RuledBack`] (two bands' backs meeting
/// along the line where they meet, the same curve of both), the walls along
/// both profiles (from the front's side to the back's, two bands' walls
/// meeting along the line from the corner to where their backs meet) and
/// across the first and last rulings. Faces `<feature>:lofted_front(i)`,
/// `lofted_back(i)`, `lofted_first(i)`, `lofted_second(i)`,
/// `lofted_start(0)` and `lofted_end(0)`.
pub(super) fn ruled_body(
    bands: &[RuledBand],
    depth: f64,
    feature: FeatureUid,
) -> Result<Vec<SurfacePatch>, String> {
    let name = |role: &str, i: usize| FaceName::new(feature, role, Some(RoleKey::Index(i as u32)));
    let back = RuledBack::new(bands, depth)?;
    let grids = (0..bands.len())
        .map(|i| BackGrid::new(&back, i))
        .collect::<Result<Vec<_>, String>>()?;
    let mut out = ruled_patches(bands, feature);
    let stations = |f: &dyn Fn(f64) -> V3, n: usize| -> Vec<V3> {
        (0..=n).map(|k| f(k as f64 / n as f64)).collect()
    };
    let last = bands.len() - 1;
    for (i, (band, grid)) in bands.iter().zip(&grids).enumerate() {
        out.push(grid.patch(name("lofted_back", i)));
        // The walls along the profiles: the front's side to the back's.
        for (role, run, row) in [
            ("lofted_first", &band.a, 0),
            ("lofted_second", &band.b, BACK_SPANS),
        ] {
            let front = smooth_poles(
                &stations(&|f| run.point(f), SPANS),
                &stations(&|f| run.derivative(f), SPANS),
            );
            out.push(smooth_ruled(
                name(role, i),
                &front,
                &smooth_poles(&grid.points[row], &grid.ds[row]),
            ));
        }
        // The walls across the first and the last ruling.
        for (role, end, k) in [("lofted_start", 0.0, 0), ("lofted_end", 1.0, SPANS)] {
            if (k == 0 && i != 0) || (k == SPANS && i != last) {
                continue;
            }
            let ruling = sub(band.b.point(end), band.a.point(end));
            let front = smooth_poles(
                &stations(&|v| band.point(end, v), BACK_SPANS),
                &[ruling; BACK_SPANS + 1],
            );
            let (points, dv) = grid.column(k);
            out.push(smooth_ruled(
                name(role, 0),
                &front,
                &smooth_poles(&points, &dv),
            ));
        }
    }
    Ok(out)
}

/// A press-brake transition's facets: the area of each flat band at the
/// depth (its two triangles), for checks.
#[cfg(test)]
pub(super) fn flat_area(t: &Transition, depth: f64) -> f64 {
    let bands = t.bands();
    let mut area = 0.0;
    for (i, b) in bands.iter().enumerate() {
        if let Band::Flat { .. } = b {
            let c = t.flat_at(&bands, i, depth);
            area += norm(cross(sub(c[1], c[0]), sub(c[2], c[0]))) / 2.0;
            area += norm(cross(sub(c[2], c[3]), sub(c[1], c[3]))) / 2.0;
        }
    }
    area
}

#[cfg(test)]
#[path = "lofted_tests.rs"]
mod tests;
