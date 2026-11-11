// SPDX-License-Identifier: MIT
//! Spatial constraint system: the solver of 3D sketches (mitcad#1520,
//! mitcad#1542), separate from the planar [`crate::System`].
//!
//! Every point has three unknown coordinates in the sketch's local,
//! orthonormal frame (millimetres). The frame itself (origin `O`, axes
//! `R`, component position `O + R p`) is the caller's and no unknown here:
//! a source vertex `V` becomes the local attachment target `R^T (V - O)`
//! before the solve, a source curve a constant [`SpatialReferenceCurve`].
//! Curves refer to their points by id: curves that share a point id are
//! joined there (shared identity); separately created points are joined
//! only by an explicit [`SpatialConstraint::Coincident`] (three
//! coordinate-difference rows, never a squared distance, whose gradient
//! vanishes where it holds).
//!
//! Unknowns (`d = b - a`, `L` a characteristic length, see below):
//!
//! | Entity | Unknowns | Internal rows | DOF |
//! |---|---|---|---|
//! | Point | `x, y, z` | none | 3 |
//! | Line `a b` | none (its points) | none | |
//! | Arc (free normal) | centre `c`, start `s`, end `e` (points) and a unit normal `n` | `abs(s-c)-abs(e-c)`, `n.(s-c)`, `n.(e-c)`, `L (n.n-1)/2` | 8 |
//! | Arc (plane-bound: a frame axis `k` as normal) | `c`, `s`, `e` | `abs(s-c)-abs(e-c)`, `s_k-c_k`, `e_k-c_k` | 6 |
//! | Circle | centre, radius `r`, `n` (free) | `L (n.n-1)/2` (free) | 6 free, 4 plane-bound |
//! | Control-point spline | its control points (degree, knots, weights fixed) | none | 3 per point |
//! | Fit-point spline | fit points and hidden control points (3 each); chord-length parameters `t_i` fixed | `p_i-C(t_i)` per interior fit point, natural end `C''(t)`, periodic seam `C'`, `C''` | 3 per fit point, + 3 per free end |
//! | Ellipse (plane-bound: a frame axis `k` as normal, mitcad#1550) | centre `c`, major point `a`, minor radius `b` | `a_k-c_k` | 6 |
//! | Elliptical arc | the ellipse's and its ends `s`, `e` | the ellipse's, `s_k-c_k`, `e_k-c_k` and `OnEllipse` for each end | 8 |
//! | Point on a line, spline or reference curve | its curve parameter `t`, kept in the curve's range | (the relation's rows) | |
//!
//! Arcs run counter-clockwise about their normal from start to end. A free
//! normal is real freedom (a half circle turns about its diameter); `n`
//! and `-n` give complementary arcs, and the branch is the one the
//! iteration starts from. A tangency at a fit-point spline's natural end
//! drops that end's three second-derivative rows.
//!
//! Relations (every residual a length in millimetres; `u`, `v` the unit
//! tangents at the shared point `P`: a line's `unit(b-a)`, an arc's or
//! circle's `unit(n x (P-c))`, a spline end's `unit(C'(t))`):
//!
//! | Relation | Rows |
//! |---|---|
//! | `Coincident(a, b)` | `b_k - a_k` for x, y, z (3) |
//! | `Fix { point, at }`, `Attach { point, target }` | `p_k - t_k` (3) |
//! | `Align { line, axis, sense }` | the two transverse differences `d_i`, `d_k` (2): parallel to the axis anywhere |
//! | `Offset { a, b, axis, value }` | `d_j - value` (1), signed |
//! | `Length { line, value }` | `abs(d) - value` (1); on a line with an alignment: `d_j - s value` (1, linear), `s` its sense |
//! | `PointOn` a line | `p - a - t (b - a)` (3), `t` in `[0, 1]` |
//! | `PointOn` an arc or circle (its full circle) | `n.(p-c)`, `abs(p-c) - abs(s-c)` (or `- r`) (2) |
//! | `PointOn` a spline or reference curve | `p - C(t)` (3), `t` in the curve's range |
//! | `PointOn` an ellipse | `p_k - c_k`, `(sqrt((X b)^2 + (Y A)^2) - A b) / L` (2), `A = abs(a-c)`, `X`, `Y` along `u = unit(a-c)` and `e_k x u` |
//! | `EllipseRadius` | `abs(a-c) - R` or `b - R` (1) |
//! | `Midpoint` of a line | `2 m - a - b` (3) |
//! | `Midpoint` of an arc | `abs(m-s) - abs(m-e)`, `n.(m-c)`, `abs(m-c) - abs(s-c)` (3); the sweep side is validated |
//! | `Midpoint` of a reference curve | `m - target` (3), the target its point halfway along |
//! | `Tangent` line and arc or circle | `u.(P-c)`, `L u.n` (2) |
//! | `Tangent` arc or circle and arc or circle | `u_a.(P-c_b)`, `L u_a.n_b` (2) |
//! | `Tangent` otherwise (lines, splines) | `L u.w1`, `L u.w2`, `w1 = unit(v x e)`, `w2 = v x w1` (2), `e` the frame axis least aligned with `v` |
//! | `Equal` lines / rounds | `abs(d_1) - abs(d_2)` / radius difference (1) |
//! | `Radius`, `Diameter` | `abs(s-c) - R` (or `r - R`), `2 abs(s-c) - D` (1) |
//! | `ArcLength` | `abs(s-c) theta - A`, `theta` the sweep in `(0, 2 pi)` (1) |
//! | `NormalAlign { curve, axis, sense }` | `L n_i`, `L n_k` (the transverse components, 2) |
//! | `OnSketchPlane(p)` | `p_z` (1) |
//! | `OnSurface { point, surface }` | the signed distance from `p` to the surface (1, closed form: plane, cylinder, cone, sphere, torus; see `spatial/surface.rs`) |
//! | `Parallel(a, b)` (mitcad#1661: lines' directions, arcs' and circles' normals) | `L u.w1`, `L u.w2` across `v` as for a general tangency (2), either sense |
//! | `Perpendicular(a, b)` | `L u.v` (1) |
//! | `Angle { a, b, value }` | `L (atan2(abs(u x v), u.v) - value)` (1), `value` in `(0, pi)` |
//! | `Symmetric { a, b, plane }` | `n.((a+b)/2) - n.o`, `w1.(b-a)`, `w2.(b-a)` (3, linear), `w1`, `w2` across the plane's normal `n` |
//! | `Concentric { a, b, normal }` | `c_b - c_a` (3, none for one centre point), with `normal` the parallel rows of the normals (2) |
//! | `Curvature { a, b, at }` (G2) | the tangency's rows and `L^2 (k_a - k_b).w1`, `L^2 (k_a - k_b).w2` (2), `k` the curvature vectors at `at` (a line's none, a round's `(c-P)/abs(c-P)^2`, a spline end's `(C' x C'') x C'/abs(C')^4`) |
//! | `SplineLength { curve, value }` | `length(C) - value` (1): Gauss-Legendre quadrature of `abs(C')` on the knot spans, the formula [`SpatialSystem::spline_length`] measures with |
//! | `TangentLength { curve, end, value }` | `abs(C'(t)) - value` (1) at the spline's first or last point |
//! | `InPlane(c)` | the planar rows of a converted constraint or dimension (or of a plane-bound shape's) on two coordinate columns of its points: x and y, or those of the plane square to another frame axis (see `spatial/plane.rs`) |
//! | `Along(Offset { a, b, dir, value })` | `dir.(b-a) - value` (1, linear; mitcad#1661: along a rotated plane's axis, or zero along its normal for a shape's points) |
//! | `Along(Line { line, dir, sense })` | `w1.d`, `w2.d` (2, linear), `w1`, `w2` across `dir`; the sense is validated |
//! | `Along(Normal { curve, dir })` | `L n.w1`, `L n.w2` (2): a free normal along `dir`, either way |
//! | `InPlane(c)` on a rotated plane ([`SpatialSystem::add_in_plane_frame`]) | the planar rows on the points' coordinates along the plane's in-plane axes, `x.p` and `y.p` (mitcad#1661) |
//!
//! (`d = b - a` of the line or the two points, `j` the axis.) Driven
//! measurements ([`SpatialSystem::line_length`], [`SpatialSystem::offset`],
//! [`SpatialSystem::radius`], [`SpatialSystem::arc_length`],
//! [`SpatialSystem::spline_length`]) add no rows. A tangency's sense (the
//! sign of `u.v` when it is added) and an alignment's are validated after
//! the solve, not solved for.
//!
//! Scale policy: a residual of unit vectors (a normal's unit row, normal
//! alignment, tangency) is multiplied by its entity's characteristic
//! length `L` (a line's length, an arc's or circle's radius, a spline's
//! control polygon; at least 1 mm, constant during one solve), so that
//! every residual is in millimetres and the absolute acceptance tolerance
//! means the same everywhere. Curve parameters are weighted by their
//! curve's length (a sliding point moves comparably to a coordinate) and
//! normals by their radius.
//!
//! The rows run on the planar solver's numerical engine: the
//! Levenberg-Marquardt and drag iterations of `solve.rs`, the sparse plans
//! and factorization, forward-mode differentiation and the rank analysis
//! of `analysis.rs`, over components split by the same helper
//! (`prepare::connected_components`). The planar direction graph
//! (`direction.rs`, angles modulo pi) sees only in-plane relations:
//! contradictory spatial axis requests are found by [`SpatialSystem`]'s own
//! structural check.
//!
//! A solve is accepted at an absolute tolerance ([`SPATIAL_TOLERANCE`],
//! 1e-6 mm by default, refined below it by the iteration), whatever the
//! size of the route, and only when the solved geometry is valid: no line
//! shorter than [`SPATIAL_MIN_SEGMENT`], no radius or spline spacing that
//! short, no arc whose ends meet, every aligned line, aligned normal and
//! tangency pointing its stored way. Anything else leaves every unknown
//! where it was (the whole system, not only the part that failed).

mod curve;
mod plane;
mod rows;
mod surface;

pub use curve::{SpatialCircle, SpatialReferenceCurve, circle_through};
pub use surface::SpatialSurface;

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::sync::Arc;

use self::curve::RefCurve;
use self::rows::{
    Crv, FrameVar, Len, Nrm, Owner, Row, RowKind, Size, SpatialEquations, SplineRef, Tan, Tp,
    ValueRef, Vars, sweep,
};
use crate::analysis::{self, DepRow};
use crate::equations::EqKind;
use crate::prepare::{Component, Equation, ValueSrc, connected_components};
use crate::scalar::V3;
use crate::solve::{self, Ctx, Equations, Work};
use crate::sparse::Rows;
use crate::spline::{SplineBasis, clamped_uniform_knots, interpolate};
use crate::system::ParamKind;
use crate::types::{Constraint, ConstraintId, EntityId, PointId, SplineEnd};

/// Default absolute acceptance tolerance of every spatial residual
/// (coordinates and lengths, millimetres).
pub const SPATIAL_TOLERANCE: f64 = 1e-6;

/// A line at most this long (millimetres) is collapsed: invalid. A length
/// dimension must be longer; so must a radius, the distance between an
/// arc's ends and between a spline's successive points. Distinct from the
/// acceptance tolerance, from path joining and from screen picking
/// distances.
pub const SPATIAL_MIN_SEGMENT: f64 = 1e-6;

/// Weight of a fitted spline's hidden control points (they move readily,
/// as in the planar sketch).
const HIDDEN_WEIGHT: f64 = 1e-2;

/// A point of a spatial system (three unknown coordinates).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpatialPointId(pub(crate) u32);

/// A line between two points of a spatial system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpatialLineId(pub(crate) u32);

/// An arc, circle, spline or reference curve of a spatial system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpatialCurveId(pub(crate) u32);

/// A constraint or dimension of a spatial system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpatialConstraintId(pub(crate) u32);

impl SpatialPointId {
    /// Index in creation order (stable for the lifetime of the system).
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl SpatialLineId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl SpatialCurveId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl SpatialConstraintId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A line or another curve, where a relation takes either.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpatialCurve {
    Line(SpatialLineId),
    Curve(SpatialCurveId),
}

/// A direction a relation takes (mitcad#1661): a line's, from its first
/// point to its second, or the normal of an arc or circle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SpatialDir {
    Line(SpatialLineId),
    Normal(SpatialCurveId),
}

impl SpatialDir {
    fn curve(self) -> SpatialCurve {
        match self {
            SpatialDir::Line(l) => SpatialCurve::Line(l),
            SpatialDir::Normal(c) => SpatialCurve::Curve(c),
        }
    }
}

/// Kind of a [`SpatialCurveId`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialEntityKind {
    /// Counter-clockwise about its normal from the start to the end point.
    Arc,
    Circle,
    /// A B-spline by control points (degree, knots and weights fixed).
    BSpline,
    /// A cubic spline through fit points.
    FittedSpline,
    /// A constant curve ([`SpatialReferenceCurve`]).
    Reference,
    /// A plane-bound ellipse (mitcad#1550), or an elliptical arc between
    /// two points on it.
    Ellipse,
}

/// An axis of the local frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// 0, 1 or 2.
    pub fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }

    /// The two other axes' indices.
    fn transverse(self) -> [usize; 2] {
        match self {
            Axis::X => [1, 2],
            Axis::Y => [0, 2],
            Axis::Z => [0, 1],
        }
    }

    /// The unit vector along the axis with a sense.
    fn unit(self, sense: AxisSense) -> [f64; 3] {
        let mut v = [0.0; 3];
        v[self.index()] = sense.sign();
        v
    }
}

/// Which way along its axis an aligned line points (from its first point
/// to its second), or an aligned normal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AxisSense {
    Positive,
    Negative,
}

impl AxisSense {
    /// 1 or -1.
    pub fn sign(self) -> f64 {
        match self {
            AxisSense::Positive => 1.0,
            AxisSense::Negative => -1.0,
        }
    }
}

/// The plane of an arc or circle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpatialNormal {
    /// A unit normal of its own (two degrees of freedom), starting along
    /// this direction (any nonzero length).
    Free([f64; 3]),
    /// A constant axis of the frame and its sense (a plane-bound arc or
    /// circle: converted from a planar sketch, or kept in a plane).
    Axis(Axis, AxisSense),
}

/// A spline's geometry for the caller (e.g. to build the kernel curve).
#[derive(Clone, Debug, PartialEq)]
pub struct SpatialSplineGeometry {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub weights: Vec<f64>,
    pub control_points: Vec<[f64; 3]>,
}

/// A constraint or dimension of a spatial system. Values are millimetres
/// in the local frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpatialConstraint {
    /// Two distinct points at one place.
    Coincident(SpatialPointId, SpatialPointId),
    /// A point held at a deliberate local anchor.
    Fix { point: SpatialPointId, at: [f64; 3] },
    /// A point attached to a target supplied from outside: `[0, 0, 0]`
    /// for the sketch origin, a source vertex mapped into the local frame
    /// ([`SpatialSystem::set_target`] when the source moves).
    Attach {
        point: SpatialPointId,
        target: [f64; 3],
    },
    /// A line parallel to an axis (anywhere, not on the axis), pointing
    /// `sense` along it.
    Align {
        line: SpatialLineId,
        axis: Axis,
        sense: AxisSense,
    },
    /// Signed difference `b - a` along an axis; may be zero or negative.
    Offset {
        a: SpatialPointId,
        b: SpatialPointId,
        axis: Axis,
        value: f64,
    },
    /// True length of a line, longer than [`SPATIAL_MIN_SEGMENT`].
    Length { line: SpatialLineId, value: f64 },
    /// A point on a curve: a line between its ends, the full circle of an
    /// arc or circle, a spline or reference curve within its parameter
    /// range (a point at an end is valid). Not a point that defines the
    /// curve.
    PointOn {
        point: SpatialPointId,
        curve: SpatialCurve,
    },
    /// A point at the middle of a line, an arc (on the arc, not on its
    /// complement) or a reference curve (halfway along it, a constant
    /// target).
    Midpoint {
        point: SpatialPointId,
        curve: SpatialCurve,
    },
    /// Two curves tangent at the point `at`: an end (or, for an arc or
    /// circle, a point on it) of each arc, circle and spline, where the
    /// tangents are taken. Lines need not pass through `at`; contact is a
    /// shared point or a coincidence of its own. The sense (whether the
    /// tangents point the same way) is kept from the geometry when added.
    Tangent {
        a: SpatialCurve,
        b: SpatialCurve,
        at: SpatialPointId,
    },
    /// Equal lengths of two lines or equal radii of two arcs or circles.
    Equal(SpatialCurve, SpatialCurve),
    /// The radius of an arc or circle.
    Radius { curve: SpatialCurveId, value: f64 },
    /// The diameter of an arc or circle.
    Diameter { curve: SpatialCurveId, value: f64 },
    /// The length of an arc.
    ArcLength { curve: SpatialCurveId, value: f64 },
    /// The normal of an arc or circle (with a free normal) along a frame
    /// axis, pointing `sense` along it.
    NormalAlign {
        curve: SpatialCurveId,
        axis: Axis,
        sense: AxisSense,
    },
    /// A point on the sketch plane: local `z = 0`.
    OnSketchPlane(SpatialPointId),
    /// A point on a constant surface (a face of a body, mitcad#1549): one
    /// row, the point keeps two degrees of freedom to slide on it
    /// ([`SpatialSystem::set_surface`] when the face moves).
    OnSurface {
        point: SpatialPointId,
        surface: SpatialSurface,
    },
    /// A planar constraint or dimension kept by a converted sketch
    /// ([`SpatialSystem::add_in_plane`]), solved on its points' x and y.
    InPlane(ConstraintId),
    /// The major (`minor` false: from the centre to the major point) or
    /// minor radius of an ellipse (mitcad#1550).
    EllipseRadius {
        curve: SpatialCurveId,
        minor: bool,
        value: f64,
    },
    /// Two directions parallel, either way along each other, with no
    /// intersection implied (mitcad#1661).
    Parallel(SpatialDir, SpatialDir),
    /// Two directions at right angles, with no intersection implied.
    Perpendicular(SpatialDir, SpatialDir),
    /// The angle between two directions, radians strictly between nothing
    /// and a half turn.
    Angle {
        a: SpatialDir,
        b: SpatialDir,
        value: f64,
    },
    /// Two distinct points mirrored across a constant plane
    /// ([`SpatialSurface::Plane`]): their middle on it, the line between
    /// them along its normal.
    Symmetric {
        a: SpatialPointId,
        b: SpatialPointId,
        plane: SpatialSurface,
    },
    /// Two arcs or circles with one centre, and with `normal` in parallel
    /// planes (their normals parallel, either way).
    Concentric {
        a: SpatialCurveId,
        b: SpatialCurveId,
        normal: bool,
    },
    /// Curvature continuity (G2) at `at`: the tangency of
    /// [`SpatialConstraint::Tangent`] (with its kept sense) and equal
    /// curvature vectors there; one curve at least is a spline.
    Curvature {
        a: SpatialCurve,
        b: SpatialCurve,
        at: SpatialPointId,
    },
    /// The length of a spline (driving).
    SplineLength { curve: SpatialCurveId, value: f64 },
    /// The magnitude of an open spline's end tangent `abs(C'(t))` at its
    /// first (`end` 0) or last (`end` 1) point, in its parameter.
    TangentLength {
        curve: SpatialCurveId,
        end: usize,
        value: f64,
    },
    /// A relation along a constant unit direction of the local frame
    /// (mitcad#1661: the axes and the normal of a rotated placement plane).
    Along(SpatialAlong),
}

/// Relations along a constant unit direction `dir` of the local frame
/// (mitcad#1661): what [`SpatialConstraint::Offset`],
/// [`SpatialConstraint::Align`] and [`SpatialConstraint::NormalAlign`] are
/// along a frame axis, along any direction (a rotated placement plane's
/// axes and normal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpatialAlong {
    /// Signed `(b - a).dir`; may be zero or negative (zero for the points
    /// of a shape bound to a rotated plane: one plane square to `dir`).
    Offset {
        a: SpatialPointId,
        b: SpatialPointId,
        dir: [f64; 3],
        value: f64,
    },
    /// A line along `dir` (anywhere), pointing `sense` along it.
    Line {
        line: SpatialLineId,
        dir: [f64; 3],
        sense: AxisSense,
    },
    /// The free normal of an arc or circle along `dir`, either way (a
    /// circle bound to a rotated plane).
    Normal {
        curve: SpatialCurveId,
        dir: [f64; 3],
    },
}

impl SpatialAlong {
    /// The unit direction.
    pub fn dir(&self) -> [f64; 3] {
        match *self {
            Self::Offset { dir, .. } | Self::Line { dir, .. } | Self::Normal { dir, .. } => dir,
        }
    }
}

impl SpatialConstraint {
    fn points(&self) -> Vec<SpatialPointId> {
        use SpatialConstraint::*;
        match *self {
            Coincident(a, b) | Offset { a, b, .. } => vec![a, b],
            Fix { point, .. }
            | Attach { point, .. }
            | PointOn { point, .. }
            | Midpoint { point, .. }
            | OnSketchPlane(point)
            | OnSurface { point, .. } => vec![point],
            Tangent { at, .. } | Curvature { at, .. } => vec![at],
            Symmetric { a, b, .. } => vec![a, b],
            Along(SpatialAlong::Offset { a, b, .. }) => vec![a, b],
            Along(_) => Vec::new(),
            Align { .. }
            | Length { .. }
            | Equal(..)
            | Radius { .. }
            | Diameter { .. }
            | ArcLength { .. }
            | NormalAlign { .. }
            | EllipseRadius { .. }
            | Parallel(..)
            | Perpendicular(..)
            | Angle { .. }
            | Concentric { .. }
            | SplineLength { .. }
            | TangentLength { .. }
            | InPlane(_) => Vec::new(),
        }
    }

    /// The lines and curves the relation uses.
    fn curves(&self) -> Vec<SpatialCurve> {
        use SpatialConstraint::*;
        match *self {
            Align { line, .. } | Length { line, .. } => vec![SpatialCurve::Line(line)],
            PointOn { curve, .. } | Midpoint { curve, .. } => vec![curve],
            Tangent { a, b, .. } | Curvature { a, b, .. } | Equal(a, b) => vec![a, b],
            Radius { curve, .. }
            | Diameter { curve, .. }
            | ArcLength { curve, .. }
            | EllipseRadius { curve, .. }
            | SplineLength { curve, .. }
            | TangentLength { curve, .. }
            | NormalAlign { curve, .. } => vec![SpatialCurve::Curve(curve)],
            Parallel(a, b) | Perpendicular(a, b) | Angle { a, b, .. } => {
                vec![a.curve(), b.curve()]
            }
            Concentric { a, b, .. } => vec![SpatialCurve::Curve(a), SpatialCurve::Curve(b)],
            Along(SpatialAlong::Line { line, .. }) => vec![SpatialCurve::Line(line)],
            Along(SpatialAlong::Normal { curve, .. }) => vec![SpatialCurve::Curve(curve)],
            Along(SpatialAlong::Offset { .. }) => Vec::new(),
            Coincident(..)
            | Fix { .. }
            | Attach { .. }
            | Offset { .. }
            | OnSketchPlane(_)
            | OnSurface { .. }
            | Symmetric { .. }
            | InPlane(_) => Vec::new(),
        }
    }
}

/// Errors of the editing operations.
#[derive(Clone, Debug, PartialEq)]
pub enum SpatialError {
    UnknownPoint(SpatialPointId),
    UnknownLine(SpatialLineId),
    UnknownCurve(SpatialCurveId),
    UnknownConstraint(SpatialConstraintId),
    /// A value or definition is out of range (a coordinate that is not
    /// finite, a length not longer than [`SPATIAL_MIN_SEGMENT`], a line or
    /// relation from a point to itself, a relation the curve kind does not
    /// take).
    InvalidValue(&'static str),
    /// The constraint has no such value.
    WrongConstraintKind {
        constraint: SpatialConstraintId,
        expected: &'static str,
    },
    /// The point, line or curve is still used by a curve or constraint.
    InUse,
    /// An in-plane relation the planar system refuses.
    InPlane(crate::Error),
}

impl fmt::Display for SpatialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpatialError::UnknownPoint(p) => write!(f, "unknown point {}", p.0),
            SpatialError::UnknownLine(l) => write!(f, "unknown line {}", l.0),
            SpatialError::UnknownCurve(c) => write!(f, "unknown curve {}", c.0),
            SpatialError::UnknownConstraint(c) => write!(f, "unknown constraint {}", c.0),
            SpatialError::InvalidValue(what) => write!(f, "invalid value: {what}"),
            SpatialError::WrongConstraintKind {
                constraint,
                expected,
            } => write!(f, "constraint {} is not {expected}", constraint.0),
            SpatialError::InUse => write!(f, "still in use"),
            SpatialError::InPlane(e) => write!(f, "in-plane relation: {e}"),
        }
    }
}

impl std::error::Error for SpatialError {}

/// Options of [`SpatialSystem::solve`] and [`SpatialSystem::drag`].
#[derive(Clone, Debug)]
pub struct SpatialOptions {
    /// Absolute acceptance tolerance (millimetres) of every residual,
    /// whatever the size of the sketch.
    pub tolerance: f64,
    /// Iteration limit of one Levenberg-Marquardt run.
    pub max_iterations: usize,
    /// Apply large value changes in steps (continuation), which keeps the
    /// geometry on the branch it started on.
    pub continuation: bool,
}

impl Default for SpatialOptions {
    fn default() -> Self {
        SpatialOptions {
            tolerance: SPATIAL_TOLERANCE,
            max_iterations: 100,
            continuation: true,
        }
    }
}

/// Outcome of a spatial solve or drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialStatus {
    /// Every relation holds within the tolerance and the geometry is valid.
    Converged,
    /// Relations contradict each other ([`SpatialSolveResult::conflicts`]):
    /// contradictory axis requests, or a dependent set of rows whose
    /// residuals cannot all vanish.
    Conflicting,
    /// No solution was found from the current geometry, and no
    /// contradiction was proven ([`SpatialIssue::Singular`] names a
    /// relation that has no gradient there).
    NotConverged,
    /// The relations hold, or would, only with invalid geometry
    /// ([`SpatialSolveResult::issues`]): a collapsed line or curve, an
    /// alignment or tangency pointing against its sense.
    Invalid,
}

/// A constraint that depends on others, with the set involved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpatialDependency {
    /// The most recently added constraint of the set.
    pub constraint: SpatialConstraintId,
    /// All constraints of the set, `constraint` included, ascending.
    pub involved: Vec<SpatialConstraintId>,
}

/// Invalid geometry or a relation that cannot be solved from where the
/// geometry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpatialIssue {
    /// A line whose ends are one point: joined by coincidences, or at most
    /// [`SPATIAL_MIN_SEGMENT`] apart.
    CollapsedLine(SpatialLineId),
    /// An aligned line pointing against its alignment's sense.
    ReversedSense {
        constraint: SpatialConstraintId,
        line: SpatialLineId,
    },
    /// A relation whose row has no gradient at the geometry (the true
    /// length of a line whose ends are at one place), so no step can make
    /// it hold from there.
    Singular(SpatialConstraintId),
    /// An arc, circle or spline without extent: a radius at most
    /// [`SPATIAL_MIN_SEGMENT`], an arc whose ends meet (a sweep collapsing
    /// to nothing or to a full turn) or whose defining points are joined,
    /// successive spline points that close, or a spline without a tangent
    /// where a tangency takes it.
    DegenerateCurve(SpatialCurveId),
    /// A tangency whose curves turned to point against the sense stored
    /// when it was added.
    ReversedTangent(SpatialConstraintId),
    /// An aligned normal pointing against its alignment's sense.
    ReversedNormal {
        constraint: SpatialConstraintId,
        curve: SpatialCurveId,
    },
    /// The midpoint of an arc on the arc's complement.
    MidpointOffArc(SpatialConstraintId),
}

/// Result of [`SpatialSystem::solve`] or [`SpatialSystem::drag`].
#[derive(Clone, Debug, PartialEq)]
pub struct SpatialSolveResult {
    pub status: SpatialStatus,
    /// Iterations of the longest run (over independent parts and steps).
    pub iterations: usize,
    /// Largest residual (millimetres) of the run that decided the status.
    pub residual: f64,
    /// Contradicting sets when the status is `Conflicting`.
    pub conflicts: Vec<SpatialDependency>,
    /// Invalid geometry (`Invalid`) or singular relations (`NotConverged`).
    pub issues: Vec<SpatialIssue>,
}

impl SpatialSolveResult {
    pub fn is_ok(&self) -> bool {
        self.status == SpatialStatus::Converged
    }

    /// All constraints of all conflicting sets, ascending.
    pub fn conflicting(&self) -> Vec<SpatialConstraintId> {
        let mut v: Vec<SpatialConstraintId> = self
            .conflicts
            .iter()
            .flat_map(|d| d.involved.iter().copied())
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// A soft goal for [`SpatialSystem::drag`]: a point pulled toward a local
/// target as far as the relations allow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialDragGoal {
    pub point: SpatialPointId,
    pub target: [f64; 3],
}

/// A soft goal for [`SpatialSystem::drag_with`]: the tangent `C'(t)` of a
/// fit-point spline at its first (`end` 0) or last (`end` 1) point, pulled
/// toward `target` (direction and magnitude, in the spline's parameter);
/// it moves where the end is free or controlled by a tangency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialTangentGoal {
    pub spline: SpatialCurveId,
    pub end: usize,
    pub target: [f64; 3],
}

/// Degrees of freedom and constraint status at the current geometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpatialAnalysis {
    /// Unknowns: three per point, the normals, radii and hidden control
    /// points of curves and the curve parameters of relations.
    pub unknowns: usize,
    /// Independent rows.
    pub rank: usize,
    /// Remaining degrees of freedom, `unknowns - rank` (redundant rows
    /// take none away).
    pub dof: usize,
    /// Every row holds within [`SPATIAL_TOLERANCE`] at the geometry.
    pub solved: bool,
    /// Points whose three coordinates are all determined, ascending; empty
    /// unless the geometry is solved and valid without conflicts.
    pub fully_constrained_points: Vec<SpatialPointId>,
    /// Lines whose both points are determined, ascending (same condition).
    pub fully_constrained_lines: Vec<SpatialLineId>,
    /// Curves whose points and own unknowns (normal, radius, hidden control
    /// points) are determined, ascending (same condition); curve parameters
    /// are auxiliary and not part of it.
    pub fully_constrained_curves: Vec<SpatialCurveId>,
    /// Constraints that follow from earlier ones (consistent). A
    /// dependency among a curve's own rows is that curve's geometry (a half
    /// circle turning about its diameter), never listed here.
    pub redundant: Vec<SpatialDependency>,
    /// Constraints that contradict earlier ones.
    pub conflicts: Vec<SpatialDependency>,
    /// Invalid geometry and singular relations.
    pub issues: Vec<SpatialIssue>,
}

impl SpatialAnalysis {
    /// Solved, valid, without conflicts and with no freedom left.
    pub fn is_fully_constrained(&self) -> bool {
        self.solved && self.dof == 0 && self.conflicts.is_empty() && self.issues.is_empty()
    }

    pub fn is_point_fully_constrained(&self, p: SpatialPointId) -> bool {
        self.fully_constrained_points.binary_search(&p).is_ok()
    }

    pub fn is_line_fully_constrained(&self, l: SpatialLineId) -> bool {
        self.fully_constrained_lines.binary_search(&l).is_ok()
    }

    pub fn is_curve_fully_constrained(&self, c: SpatialCurveId) -> bool {
        self.fully_constrained_curves.binary_search(&c).is_ok()
    }

    /// Ids of the redundant constraints, ascending.
    pub fn redundant_constraints(&self) -> Vec<SpatialConstraintId> {
        let mut v: Vec<SpatialConstraintId> = self.redundant.iter().map(|d| d.constraint).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// All constraints of all conflicting sets, ascending.
    pub fn conflicting_constraints(&self) -> Vec<SpatialConstraintId> {
        let mut v: Vec<SpatialConstraintId> = self
            .conflicts
            .iter()
            .flat_map(|d| d.involved.iter().copied())
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// The plane of an arc or circle as stored.
#[derive(Clone, Copy, Debug)]
enum NormalRec {
    /// The first of three columns.
    Free(usize),
    Axis(Axis, AxisSense),
}

/// A curve; points by index, columns by global index.
#[derive(Clone, Debug)]
enum CurveData {
    Arc {
        c: usize,
        s: usize,
        e: usize,
        normal: NormalRec,
    },
    Circle {
        c: usize,
        /// The radius column.
        r: usize,
        normal: NormalRec,
    },
    BSpline {
        basis: Arc<SplineBasis>,
        ctrl: Vec<usize>,
    },
    Fitted {
        basis: Arc<SplineBasis>,
        fit: Vec<usize>,
        /// The first column of each hidden control point.
        hidden: Vec<usize>,
        params: Vec<f64>,
        ends: [SplineEnd; 2],
    },
    Reference(Arc<RefCurve>),
    /// A plane-bound ellipse (mitcad#1550): centre `c`, the end `a` of its
    /// major axis in the plane through the centre square to `axis`, the
    /// minor radius column `b`; an elliptical arc runs between `ends` on
    /// it, counter-clockwise about `axis`.
    Ellipse {
        c: usize,
        a: usize,
        b: usize,
        axis: Axis,
        ends: Option<[usize; 2]>,
        /// On a rotated plane (mitcad#1661): the plane's unit normal, in
        /// place of `axis`.
        dir: Option<[f64; 3]>,
    },
}

impl CurveData {
    fn kind(&self) -> SpatialEntityKind {
        match self {
            CurveData::Arc { .. } => SpatialEntityKind::Arc,
            CurveData::Circle { .. } => SpatialEntityKind::Circle,
            CurveData::BSpline { .. } => SpatialEntityKind::BSpline,
            CurveData::Fitted { .. } => SpatialEntityKind::FittedSpline,
            CurveData::Reference(_) => SpatialEntityKind::Reference,
            CurveData::Ellipse { .. } => SpatialEntityKind::Ellipse,
        }
    }

    /// The points that define the curve.
    fn points(&self) -> Vec<usize> {
        match self {
            CurveData::Arc { c, s, e, .. } => vec![*c, *s, *e],
            CurveData::Circle { c, .. } => vec![*c],
            CurveData::BSpline { ctrl, .. } => ctrl.clone(),
            CurveData::Fitted { fit, .. } => fit.clone(),
            CurveData::Reference(_) => Vec::new(),
            CurveData::Ellipse { c, a, ends, .. } => {
                let mut v = vec![*c, *a];
                v.extend(ends.iter().flatten());
                v
            }
        }
    }

    /// The columns the curve owns (its normal, radius, hidden control
    /// points).
    fn own_columns(&self) -> Vec<usize> {
        let normal = |n: &NormalRec| match n {
            NormalRec::Free(c) => vec![*c, c + 1, c + 2],
            NormalRec::Axis(..) => Vec::new(),
        };
        match self {
            CurveData::Arc { normal: n, .. } => normal(n),
            CurveData::Circle { r, normal: n, .. } => {
                let mut v = vec![*r];
                v.extend(normal(n));
                v
            }
            CurveData::Fitted { hidden, .. } => hidden.iter().flat_map(|&h| h..h + 3).collect(),
            CurveData::Ellipse { b, .. } => vec![*b],
            CurveData::BSpline { .. } | CurveData::Reference(_) => Vec::new(),
        }
    }

    fn is_round(&self) -> bool {
        matches!(self, CurveData::Arc { .. } | CurveData::Circle { .. })
    }

    fn is_spline(&self) -> bool {
        matches!(self, CurveData::BSpline { .. } | CurveData::Fitted { .. })
    }
}

#[derive(Clone, Debug)]
struct ConstraintRec {
    def: SpatialConstraint,
    /// Values at which the constraint last held (for continuation).
    last: Vec<f64>,
    /// Columns the relation owns (a curve parameter).
    params: Vec<usize>,
    /// A tangency's sense: the sign of `u.v` when it was added.
    sense: f64,
}

/// The rows and components, cached until the structure changes (points,
/// curves or constraints added or removed); new values keep it.
#[derive(Clone, Debug)]
struct Prepared {
    /// Relations' rows in creation order, then the curves' internal rows
    /// (the rank analysis's order: a dependency then falls on a curve's
    /// own row, which tells a curve's geometry from a redundant relation).
    rows: Vec<Row>,
    comps: Vec<Component>,
    /// Live columns that no row uses, ascending.
    free: Vec<usize>,
}

impl Prepared {
    fn update_value(&mut self, constraint: usize, k: usize, v: f64) {
        for row in &mut self.rows {
            if let Some(s) = row.src
                && s.constraint == constraint
                && s.k == k
            {
                row.value = s.factor * v;
            }
        }
    }
}

/// A row of `owner` whose kind `f` builds over a fresh local numbering.
fn row(owner: Owner, f: impl FnOnce(&mut Vars) -> RowKind) -> Row {
    let mut v = Vars::default();
    let kind = f(&mut v);
    Row::new(owner, v, kind)
}

fn finite3(v: [f64; 3]) -> bool {
    v.iter().all(|x| x.is_finite())
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    curve::norm([b[0] - a[0], b[1] - a[1], b[2] - a[2]])
}

/// Two unit directions across a unit normal, at right angles to each
/// other (mitcad#1661: a symmetry's rows).
fn plane_across(n: [f64; 3]) -> [[f64; 3]; 2] {
    let k = (0..3)
        .min_by(|&x, &y| n[x].abs().total_cmp(&n[y].abs()))
        .unwrap_or(0);
    let mut e = [0.0; 3];
    e[k] = 1.0;
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let w1 = cross(n, e);
    let len = curve::norm(w1);
    let w1 = w1.map(|x| x / len);
    [w1, cross(n, w1)]
}

/// Merges dependencies of the same constraint.
fn merge(mut deps: Vec<SpatialDependency>) -> Vec<SpatialDependency> {
    deps.sort_by_key(|d| d.constraint);
    let mut out: Vec<SpatialDependency> = Vec::new();
    for d in deps {
        match out.last_mut() {
            Some(last) if last.constraint == d.constraint => {
                last.involved.extend(d.involved);
                last.involved.sort_unstable();
                last.involved.dedup();
            }
            _ => out.push(d),
        }
    }
    out
}

fn dependency(mut involved: Vec<usize>) -> SpatialDependency {
    involved.sort_unstable();
    involved.dedup();
    SpatialDependency {
        constraint: SpatialConstraintId(*involved.last().expect("a set") as u32),
        involved: involved
            .into_iter()
            .map(|c| SpatialConstraintId(c as u32))
            .collect(),
    }
}

/// Dependent rows as constraints' dependencies: with `conflicting` the
/// inconsistent sets (beyond `tol`) and the owners of rows without a
/// gradient that do not hold (singular, not a relation between rows),
/// otherwise the consistent (redundant) sets. A curve's own row that
/// depends on another of its own rows is that curve's geometry and no
/// relation's; one that depends on relations alone is blamed on the newest
/// of them.
fn dependencies(
    rows: &[Row],
    comp: &Component,
    jac: &Rows,
    deps: &[DepRow],
    tol: f64,
    conflicting: bool,
) -> (Vec<SpatialDependency>, Vec<Owner>) {
    let owner = |r: usize| rows[comp.eqs[r]].owner;
    let mut by: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut singular = Vec::new();
    for d in deps {
        let inconsistent = d.inconsistency.abs() > tol;
        if jac.row(d.row).1.iter().all(|&v| v == 0.0) {
            if conflicting && inconsistent {
                singular.push(owner(d.row));
            }
            continue;
        }
        if inconsistent != conflicting {
            continue;
        }
        let others: Vec<Owner> = d.coeffs.iter().map(|&(r, _)| owner(r)).collect();
        if let Owner::Curve(c) = owner(d.row)
            && !conflicting
            && others.contains(&Owner::Curve(c))
        {
            continue;
        }
        let mut involved: Vec<usize> = others
            .iter()
            .filter_map(|o| match o {
                Owner::Constraint(c) => Some(*c),
                Owner::Curve(_) => None,
            })
            .collect();
        let main = match owner(d.row) {
            Owner::Constraint(c) => c,
            Owner::Curve(_) => match involved.iter().max() {
                Some(&c) => c,
                None => continue,
            },
        };
        involved.push(main);
        by.entry(main).or_default().extend(involved);
    }
    let found = by
        .into_iter()
        .map(|(main, mut involved)| {
            involved.sort_unstable();
            involved.dedup();
            SpatialDependency {
                constraint: SpatialConstraintId(main as u32),
                involved: involved
                    .into_iter()
                    .map(|c| SpatialConstraintId(c as u32))
                    .collect(),
            }
        })
        .collect();
    (found, singular)
}

/// Singular rows' owners as issues.
fn singular_issues(owners: Vec<Owner>) -> Vec<SpatialIssue> {
    let mut v: Vec<SpatialIssue> = owners
        .into_iter()
        .map(|o| match o {
            Owner::Constraint(c) => SpatialIssue::Singular(SpatialConstraintId(c as u32)),
            Owner::Curve(c) => SpatialIssue::DegenerateCurve(SpatialCurveId(c as u32)),
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// A spatial constraint system (3D sketches). See the module
/// documentation.
///
/// ```
/// use mitcad_solver::{
///     Axis, AxisSense, SpatialConstraint, SpatialOptions, SpatialSystem,
/// };
///
/// let mut s = SpatialSystem::new();
/// let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
/// let b = s.add_point([38.0, 1.0, -2.0]).unwrap();
/// let ab = s.add_line(a, b).unwrap();
/// s.add_constraint(SpatialConstraint::Attach { point: a, target: [0.0; 3] })
///     .unwrap();
/// s.add_constraint(SpatialConstraint::Align {
///     line: ab,
///     axis: Axis::X,
///     sense: AxisSense::Positive,
/// })
/// .unwrap();
/// s.add_constraint(SpatialConstraint::Length { line: ab, value: 40.0 }).unwrap();
/// assert!(s.solve(&SpatialOptions::default()).is_ok());
/// let p = s.point(b).unwrap();
/// assert!((p[0] - 40.0).abs() < 1e-9 && p[1].abs() < 1e-9 && p[2].abs() < 1e-9);
/// assert!(s.analyze().is_fully_constrained());
/// ```
#[derive(Clone, Debug, Default)]
pub struct SpatialSystem {
    /// Every unknown: point coordinates, normals, radii, hidden control
    /// points and curve parameters (also of removed entities; no row uses
    /// those).
    params: Vec<f64>,
    /// Weight of each unknown in minimum-norm steps.
    weights: Vec<f64>,
    /// The range a curve parameter is kept in.
    bounds: Vec<Option<(f64, f64)>>,
    /// The first column (x) of each point; `None` once removed.
    points: Vec<Option<usize>>,
    lines: Vec<Option<[usize; 2]>>,
    curves: Vec<Option<CurveData>>,
    constraints: Vec<Option<ConstraintRec>>,
    /// The in-plane relations' planar shadow.
    plane: plane::Shadow,
    prepared: Option<Prepared>,
}

impl SpatialSystem {
    pub fn new() -> Self {
        Self::default()
    }

    fn invalidate(&mut self) {
        self.prepared = None;
    }

    /// New columns with these values (one weight and range for all).
    fn new_columns(&mut self, values: &[f64], weight: f64, bounds: Option<(f64, f64)>) -> usize {
        let first = self.params.len();
        for &v in values {
            self.params.push(v);
            self.weights.push(weight);
            self.bounds.push(bounds);
        }
        first
    }

    fn pidx(&self, p: SpatialPointId) -> Result<usize, SpatialError> {
        match self.points.get(p.index()) {
            Some(Some(_)) => Ok(p.index()),
            _ => Err(SpatialError::UnknownPoint(p)),
        }
    }

    /// The first column of a live point.
    fn pc(&self, p: usize) -> usize {
        self.points[p].expect("live point")
    }

    fn lidx(&self, l: SpatialLineId) -> Result<[usize; 2], SpatialError> {
        match self.lines.get(l.index()) {
            Some(Some(ends)) => Ok(*ends),
            _ => Err(SpatialError::UnknownLine(l)),
        }
    }

    fn cidx(&self, c: SpatialCurveId) -> Result<&CurveData, SpatialError> {
        match self.curves.get(c.index()) {
            Some(Some(data)) => Ok(data),
            _ => Err(SpatialError::UnknownCurve(c)),
        }
    }

    /// A live curve's data (checked when the relation was added).
    fn data(&self, c: SpatialCurveId) -> &CurveData {
        self.curves[c.index()].as_ref().expect("live curve")
    }

    fn rec(&self, id: SpatialConstraintId) -> Result<&ConstraintRec, SpatialError> {
        match self.constraints.get(id.index()) {
            Some(Some(r)) => Ok(r),
            _ => Err(SpatialError::UnknownConstraint(id)),
        }
    }

    fn col3(&self, col: usize) -> [f64; 3] {
        [self.params[col], self.params[col + 1], self.params[col + 2]]
    }

    fn xyz(&self, p: usize) -> [f64; 3] {
        self.col3(self.pc(p))
    }

    /// `b - a` of two points.
    fn delta(&self, a: usize, b: usize) -> [f64; 3] {
        let (pa, pb) = (self.xyz(a), self.xyz(b));
        [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]]
    }

    fn live_constraints(&self) -> impl Iterator<Item = (usize, &ConstraintRec)> {
        self.constraints
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|c| (i, c)))
    }

    fn live_lines(&self) -> impl Iterator<Item = (usize, [usize; 2])> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.map(|ends| (i, ends)))
    }

    fn live_curves(&self) -> impl Iterator<Item = (usize, &CurveData)> {
        self.curves
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|c| (i, c)))
    }

    /// Every column of live points, curves and relations, ascending.
    fn live_columns(&self) -> Vec<usize> {
        let mut cols: Vec<usize> = self
            .points
            .iter()
            .flatten()
            .flat_map(|&c| c..c + 3)
            .collect();
        for (_, data) in self.live_curves() {
            cols.extend(data.own_columns());
        }
        for (_, rec) in self.live_constraints() {
            cols.extend(&rec.params);
        }
        cols.sort_unstable();
        cols.dedup();
        cols
    }

    // Points and lines.

    /// Adds a free point (three degrees of freedom) at local coordinates.
    pub fn add_point(&mut self, xyz: [f64; 3]) -> Result<SpatialPointId, SpatialError> {
        if !finite3(xyz) {
            return Err(SpatialError::InvalidValue("a coordinate is not finite"));
        }
        let col = self.new_columns(&xyz, 1.0, None);
        self.points.push(Some(col));
        self.invalidate();
        Ok(SpatialPointId(self.points.len() as u32 - 1))
    }

    /// Where a point is (local coordinates).
    pub fn point(&self, p: SpatialPointId) -> Option<[f64; 3]> {
        self.pidx(p).ok().map(|i| self.xyz(i))
    }

    /// Moves a point (a start position; it fixes nothing).
    pub fn set_point(&mut self, p: SpatialPointId, xyz: [f64; 3]) -> Result<(), SpatialError> {
        let i = self.pidx(p)?;
        if !finite3(xyz) {
            return Err(SpatialError::InvalidValue("a coordinate is not finite"));
        }
        let col = self.pc(i);
        self.params[col..col + 3].copy_from_slice(&xyz);
        self.plane.sync(&self.params);
        Ok(())
    }

    /// All points in creation order.
    pub fn points(&self) -> impl Iterator<Item = SpatialPointId> {
        self.points
            .iter()
            .enumerate()
            .filter(|(_, live)| live.is_some())
            .map(|(i, _)| SpatialPointId(i as u32))
    }

    /// Removes a point that no line, curve or constraint uses.
    pub fn remove_point(&mut self, p: SpatialPointId) -> Result<(), SpatialError> {
        let i = self.pidx(p)?;
        let by_line = self.live_lines().any(|(_, ends)| ends.contains(&i));
        let by_curve = self.live_curves().any(|(_, c)| c.points().contains(&i));
        let by_constraint = self
            .live_constraints()
            .any(|(_, c)| c.def.points().contains(&p));
        if by_line || by_curve || by_constraint || self.plane.uses_point(i) {
            return Err(SpatialError::InUse);
        }
        self.points[i] = None;
        self.plane.remove_point(i);
        self.invalidate();
        Ok(())
    }

    /// Adds a line from `a` to `b` (two distinct points; lines that share a
    /// point are joined there).
    pub fn add_line(
        &mut self,
        a: SpatialPointId,
        b: SpatialPointId,
    ) -> Result<SpatialLineId, SpatialError> {
        let (ia, ib) = (self.pidx(a)?, self.pidx(b)?);
        if ia == ib {
            return Err(SpatialError::InvalidValue(
                "a line needs two distinct points",
            ));
        }
        self.lines.push(Some([ia, ib]));
        self.invalidate();
        Ok(SpatialLineId(self.lines.len() as u32 - 1))
    }

    /// The first and second point of a line.
    pub fn line(&self, l: SpatialLineId) -> Option<[SpatialPointId; 2]> {
        let [a, b] = self.lidx(l).ok()?;
        Some([SpatialPointId(a as u32), SpatialPointId(b as u32)])
    }

    /// All lines in creation order.
    pub fn lines(&self) -> impl Iterator<Item = SpatialLineId> {
        self.live_lines().map(|(i, _)| SpatialLineId(i as u32))
    }

    /// Removes a line that no constraint uses (its points stay).
    pub fn remove_line(&mut self, l: SpatialLineId) -> Result<(), SpatialError> {
        self.lidx(l)?;
        if self.plane.uses_line(l.index())
            || self
                .live_constraints()
                .any(|(_, c)| c.def.curves().contains(&SpatialCurve::Line(l)))
        {
            return Err(SpatialError::InUse);
        }
        self.lines[l.index()] = None;
        self.plane.remove_line(l.index());
        self.invalidate();
        Ok(())
    }

    // Curves.

    fn add_curve(&mut self, data: CurveData) -> SpatialCurveId {
        self.curves.push(Some(data));
        self.invalidate();
        SpatialCurveId(self.curves.len() as u32 - 1)
    }

    fn normal_rec(&mut self, normal: SpatialNormal) -> Result<NormalRec, SpatialError> {
        match normal {
            SpatialNormal::Free(v) => {
                let len = curve::norm(v);
                if !finite3(v) || len <= 0.0 || !len.is_finite() {
                    return Err(SpatialError::InvalidValue(
                        "a normal must be a finite, nonzero vector",
                    ));
                }
                let n = v.map(|x| x / len);
                Ok(NormalRec::Free(self.new_columns(&n, 1.0, None)))
            }
            SpatialNormal::Axis(axis, sense) => Ok(NormalRec::Axis(axis, sense)),
        }
    }

    /// Adds an arc counter-clockwise about its normal from `start` to `end`
    /// around `centre` (three distinct points; the end points stay at equal
    /// distance from the centre, in the plane through it across the
    /// normal). A free normal gives it 8 degrees of freedom with free
    /// points, a frame axis 6.
    pub fn add_arc(
        &mut self,
        centre: SpatialPointId,
        start: SpatialPointId,
        end: SpatialPointId,
        normal: SpatialNormal,
    ) -> Result<SpatialCurveId, SpatialError> {
        let (c, s, e) = (self.pidx(centre)?, self.pidx(start)?, self.pidx(end)?);
        if c == s || c == e || s == e {
            return Err(SpatialError::InvalidValue(
                "an arc needs three distinct points",
            ));
        }
        let normal = self.normal_rec(normal)?;
        Ok(self.add_curve(CurveData::Arc { c, s, e, normal }))
    }

    /// Adds a circle with a centre point, a radius (longer than
    /// [`SPATIAL_MIN_SEGMENT`]) and a normal.
    pub fn add_circle(
        &mut self,
        centre: SpatialPointId,
        radius: f64,
        normal: SpatialNormal,
    ) -> Result<SpatialCurveId, SpatialError> {
        let c = self.pidx(centre)?;
        if !radius.is_finite() || radius <= SPATIAL_MIN_SEGMENT {
            return Err(SpatialError::InvalidValue(
                "a radius must be finite and longer than the shortest line",
            ));
        }
        let normal = self.normal_rec(normal)?;
        let r = self.new_columns(&[radius], 1.0, None);
        Ok(self.add_curve(CurveData::Circle { c, r, normal }))
    }

    /// Adds a plane-bound ellipse (mitcad#1550): around `centre`, its major
    /// axis to `major` (kept in the plane through the centre square to
    /// `axis`), a minor radius (longer than [`SPATIAL_MIN_SEGMENT`]) that
    /// the solve finds like any other unknown. With `ends`, an elliptical
    /// arc counter-clockwise about `axis` from the first to the second end
    /// (both kept on the ellipse): 6 degrees of freedom for a full one with
    /// free points, 8 for an arc.
    pub fn add_ellipse(
        &mut self,
        centre: SpatialPointId,
        major: SpatialPointId,
        minor_radius: f64,
        axis: Axis,
        ends: Option<[SpatialPointId; 2]>,
    ) -> Result<SpatialCurveId, SpatialError> {
        let (c, a) = (self.pidx(centre)?, self.pidx(major)?);
        if c == a {
            return Err(SpatialError::InvalidValue(
                "an ellipse needs a centre and a major point that are two points",
            ));
        }
        if !minor_radius.is_finite() || minor_radius <= SPATIAL_MIN_SEGMENT {
            return Err(SpatialError::InvalidValue(
                "a minor radius must be finite and longer than the shortest line",
            ));
        }
        let ends = match ends {
            Some([s, e]) => {
                let (s, e) = (self.pidx(s)?, self.pidx(e)?);
                if s == e || [s, e].iter().any(|p| *p == c || *p == a) {
                    return Err(SpatialError::InvalidValue(
                        "an elliptical arc needs two ends apart from its centre and major point",
                    ));
                }
                Some([s, e])
            }
            None => None,
        };
        let b = self.new_columns(&[minor_radius], 1.0, None);
        Ok(self.add_curve(CurveData::Ellipse {
            c,
            a,
            b,
            axis,
            ends,
            dir: None,
        }))
    }

    /// Adds a full ellipse on a rotated plane (mitcad#1661): as
    /// [`SpatialSystem::add_ellipse`], its plane square to the unit
    /// `normal` in place of a frame axis.
    pub fn add_ellipse_along(
        &mut self,
        centre: SpatialPointId,
        major: SpatialPointId,
        minor_radius: f64,
        normal: [f64; 3],
    ) -> Result<SpatialCurveId, SpatialError> {
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if !finite3(normal) || (len - 1.0).abs() > 1e-9 {
            return Err(SpatialError::InvalidValue(
                "an ellipse's plane takes a unit normal",
            ));
        }
        let id = self.add_ellipse(centre, major, minor_radius, Axis::Z, None)?;
        if let Some(Some(CurveData::Ellipse { dir, .. })) = self.curves.get_mut(id.index()) {
            *dir = Some(normal);
        }
        self.invalidate();
        Ok(id)
    }

    /// An ellipse's major radius (from its centre to its major point) and
    /// minor radius, as solved.
    pub fn ellipse_radii(&self, c: SpatialCurveId) -> Option<(f64, f64)> {
        match *self.cidx(c).ok()? {
            CurveData::Ellipse { c, a, b, .. } => {
                Some((dist(self.xyz(c), self.xyz(a)), self.params[b]))
            }
            _ => None,
        }
    }

    /// Adds a B-spline by control points. `weights` default to 1 (a
    /// non-rational spline) and `knots` to a clamped uniform knot vector on
    /// [0, 1]. Degree, weights and knots stay fixed; the control points
    /// move.
    pub fn add_bspline(
        &mut self,
        degree: usize,
        control_points: &[SpatialPointId],
        weights: Option<&[f64]>,
        knots: Option<&[f64]>,
    ) -> Result<SpatialCurveId, SpatialError> {
        let n = control_points.len();
        if degree == 0 || n < degree + 1 {
            return Err(SpatialError::InvalidValue(
                "a spline needs degree >= 1 and degree + 1 control points",
            ));
        }
        let ctrl = control_points
            .iter()
            .map(|&p| self.pidx(p))
            .collect::<Result<Vec<_>, _>>()?;
        let positive = |x: f64| x > 0.0 && x.is_finite();
        let weights = match weights {
            Some(w) if w.len() != n || !w.iter().all(|&x| positive(x)) => {
                return Err(SpatialError::InvalidValue(
                    "one positive weight per control point",
                ));
            }
            Some(w) => w.to_vec(),
            None => vec![1.0; n],
        };
        let knots = match knots {
            Some(k)
                if k.len() != n + degree + 1
                    || k.iter().any(|x| !x.is_finite())
                    || k.windows(2).any(|w| w[1] < w[0]) =>
            {
                return Err(SpatialError::InvalidValue(
                    "knots: n + degree + 1 non-decreasing values",
                ));
            }
            Some(k) => k.to_vec(),
            None => clamped_uniform_knots(n, degree),
        };
        if knots[n] <= knots[degree] {
            return Err(SpatialError::InvalidValue("empty spline parameter range"));
        }
        let basis = Arc::new(SplineBasis {
            degree,
            knots,
            weights,
        });
        Ok(self.add_curve(CurveData::BSpline { basis, ctrl }))
    }

    /// Adds a cubic spline through the fit points (at least two, a closed
    /// one three), as in the planar sketch: its control points are hidden
    /// unknowns that follow the fit points, and the fit points' parameters
    /// (chord length) are fixed when it is added. [`SplineEnd::Natural`]
    /// ends have no curvature (dropped where a tangency takes the end),
    /// [`SplineEnd::Free`] ends a free tangent, [`SplineEnd::Periodic`] at
    /// both ends closes it through the first fit point (listed once).
    pub fn add_fitted_spline(
        &mut self,
        fit_points: &[SpatialPointId],
        ends: [SplineEnd; 2],
    ) -> Result<SpatialCurveId, SpatialError> {
        let closed = match ends {
            [SplineEnd::Periodic, SplineEnd::Periodic] => true,
            [SplineEnd::Periodic, _] | [_, SplineEnd::Periodic] => {
                return Err(SpatialError::InvalidValue(
                    "a closed spline is periodic at both ends",
                ));
            }
            _ => false,
        };
        let fit = fit_points
            .iter()
            .map(|&p| self.pidx(p))
            .collect::<Result<Vec<_>, _>>()?;
        let coords: Vec<[f64; 3]> = fit.iter().map(|&p| self.xyz(p)).collect();
        let mut spaced = coords
            .windows(2)
            .all(|w| dist(w[0], w[1]) > SPATIAL_MIN_SEGMENT);
        if closed && let (Some(&a), Some(&b)) = (coords.first(), coords.last()) {
            spaced &= dist(a, b) > SPATIAL_MIN_SEGMENT;
        }
        let it =
            interpolate(&coords, closed)
                .filter(|_| spaced)
                .ok_or(SpatialError::InvalidValue(if closed {
                    "fit points of a closed spline: at least three, successive points apart"
                } else {
                    "fit points: at least two, successive points apart"
                }))?;
        let hidden: Vec<usize> = it.ctrl[1..it.ctrl.len() - 1]
            .iter()
            .map(|c| self.new_columns(c, HIDDEN_WEIGHT, None))
            .collect();
        Ok(self.add_curve(CurveData::Fitted {
            basis: Arc::new(it.basis),
            fit,
            hidden,
            params: it.params,
            ends,
        }))
    }

    /// Adds a constant reference curve (a source edge or curve in the local
    /// frame) for points to lie on or at the middle of.
    pub fn add_reference_curve(
        &mut self,
        curve: SpatialReferenceCurve,
    ) -> Result<SpatialCurveId, SpatialError> {
        let rc = RefCurve::new(curve).map_err(SpatialError::InvalidValue)?;
        Ok(self.add_curve(CurveData::Reference(Arc::new(rc))))
    }

    /// Replaces a reference curve (its source moved): the points on it and
    /// at its middle follow on the next solve.
    pub fn set_reference_curve(
        &mut self,
        id: SpatialCurveId,
        curve: SpatialReferenceCurve,
    ) -> Result<(), SpatialError> {
        if !matches!(self.cidx(id)?, CurveData::Reference(_)) {
            return Err(SpatialError::InvalidValue("not a reference curve"));
        }
        let rc = RefCurve::new(curve).map_err(SpatialError::InvalidValue)?;
        let ((lo, hi), bounded) = rc.domain();
        // Points on it keep their parameters, inside the new range.
        for rec in self.constraints.iter().flatten() {
            if rec.def.curves().contains(&SpatialCurve::Curve(id))
                && let Some(&t) = rec.params.first()
            {
                self.bounds[t] = bounded.then_some((lo, hi));
                if bounded {
                    self.params[t] = self.params[t].clamp(lo, hi);
                }
            }
        }
        self.curves[id.index()] = Some(CurveData::Reference(Arc::new(rc)));
        self.invalidate();
        Ok(())
    }

    /// Removes a curve that no constraint uses (its points stay).
    pub fn remove_curve(&mut self, c: SpatialCurveId) -> Result<(), SpatialError> {
        self.cidx(c)?;
        if self.plane.uses_curve(c.index())
            || self
                .live_constraints()
                .any(|(_, r)| r.def.curves().contains(&SpatialCurve::Curve(c)))
        {
            return Err(SpatialError::InUse);
        }
        self.curves[c.index()] = None;
        self.plane.remove_curve(c.index());
        self.invalidate();
        Ok(())
    }

    /// All curves in creation order.
    pub fn curves(&self) -> impl Iterator<Item = SpatialCurveId> {
        self.live_curves().map(|(i, _)| SpatialCurveId(i as u32))
    }

    pub fn curve_kind(&self, c: SpatialCurveId) -> Option<SpatialEntityKind> {
        self.cidx(c).ok().map(CurveData::kind)
    }

    /// The points that define a curve: an arc's centre, start and end, a
    /// circle's centre, a spline's control or fit points (none for a
    /// reference curve).
    pub fn curve_points(&self, c: SpatialCurveId) -> Option<Vec<SpatialPointId>> {
        let data = self.cidx(c).ok()?;
        Some(
            data.points()
                .into_iter()
                .map(|p| SpatialPointId(p as u32))
                .collect(),
        )
    }

    fn normal_of(&self, n: NormalRec) -> [f64; 3] {
        match n {
            NormalRec::Free(col) => self.col3(col),
            NormalRec::Axis(axis, sense) => axis.unit(sense),
        }
    }

    /// The unit normal of an arc or circle.
    pub fn normal(&self, c: SpatialCurveId) -> Option<[f64; 3]> {
        match *self.cidx(c).ok()? {
            CurveData::Arc { normal, .. } | CurveData::Circle { normal, .. } => {
                let n = self.normal_of(normal);
                let len = curve::norm(n);
                (len > 0.0).then(|| n.map(|x| x / len))
            }
            _ => None,
        }
    }

    /// The radius of an arc or circle (a driven measurement).
    pub fn radius(&self, c: SpatialCurveId) -> Option<f64> {
        match *self.cidx(c).ok()? {
            CurveData::Arc { c, s, .. } => Some(dist(self.xyz(c), self.xyz(s))),
            CurveData::Circle { r, .. } => Some(self.params[r]),
            _ => None,
        }
    }

    /// The sweep of an arc (radians, in `(0, 2 pi]`).
    fn arc_sweep(&self, data: &CurveData) -> Option<f64> {
        let CurveData::Arc { c, s, e, normal } = *data else {
            return None;
        };
        let cv = self.xyz(c);
        let rs: V3<f64> = V3::cst(self.xyz(s)).sub(V3::cst(cv));
        let re: V3<f64> = V3::cst(self.xyz(e)).sub(V3::cst(cv));
        Some(sweep(V3::cst(self.normal_of(normal)).unit(), rs, re))
    }

    /// The length of an arc, or a circle's circumference (driven).
    pub fn arc_length(&self, c: SpatialCurveId) -> Option<f64> {
        let data = self.cidx(c).ok()?;
        match data {
            CurveData::Arc { .. } => Some(self.radius(c)? * self.arc_sweep(data)?),
            CurveData::Circle { .. } => Some(self.radius(c)? * std::f64::consts::TAU),
            _ => None,
        }
    }

    fn spline_basis<'a>(&self, data: &'a CurveData) -> &'a Arc<SplineBasis> {
        match data {
            CurveData::BSpline { basis, .. } | CurveData::Fitted { basis, .. } => basis,
            _ => unreachable!("validated as spline"),
        }
    }

    /// The first column of each control point of a spline.
    fn ctrl_columns(&self, data: &CurveData) -> Vec<usize> {
        match data {
            CurveData::BSpline { ctrl, .. } => ctrl.iter().map(|&p| self.pc(p)).collect(),
            CurveData::Fitted {
                fit, hidden, ends, ..
            } => {
                let last = if ends[0] == SplineEnd::Periodic {
                    fit[0]
                } else {
                    fit[fit.len() - 1]
                };
                let mut v = vec![self.pc(fit[0])];
                v.extend(hidden);
                v.push(self.pc(last));
                v
            }
            _ => Vec::new(),
        }
    }

    /// The point of a spline at `t`.
    fn spline_at(&self, data: &CurveData, t: f64) -> [f64; 3] {
        let cp: Vec<V3<f64>> = self
            .ctrl_columns(data)
            .iter()
            .map(|&c| V3::cst(self.col3(c)))
            .collect();
        self.spline_basis(data).eval(&cp, t, 0).p.val()
    }

    /// The length of a spline or a reference curve (a driven measurement).
    pub fn spline_length(&self, c: SpatialCurveId) -> Option<f64> {
        let data = self.cidx(c).ok()?;
        match data {
            CurveData::Reference(rc) => Some(rc.length()),
            d if d.is_spline() => {
                // The rule a driving length's row uses (mitcad#1661).
                let cp: Vec<V3<f64>> = self
                    .ctrl_columns(d)
                    .iter()
                    .map(|&col| V3::cst(self.col3(col)))
                    .collect();
                let basis = self.spline_basis(d);
                Some(rows::spline_length(basis, |t| basis.eval(&cp, t, 1).d1))
            }
            _ => None,
        }
    }

    /// A spline's degree, knots, weights and control points (hidden ones
    /// included).
    pub fn spline(&self, c: SpatialCurveId) -> Option<SpatialSplineGeometry> {
        let data = self.cidx(c).ok()?;
        if !data.is_spline() {
            return None;
        }
        let basis = self.spline_basis(data);
        Some(SpatialSplineGeometry {
            degree: basis.degree,
            knots: basis.knots.clone(),
            weights: basis.weights.clone(),
            control_points: self
                .ctrl_columns(data)
                .iter()
                .map(|&c| self.col3(c))
                .collect(),
        })
    }

    /// The end (0 or 1) of an open spline at point `at` (by identity or
    /// coincidence); `None` when `at` is at neither end or the spline is
    /// closed or has unclamped knots.
    fn spline_end(&self, data: &CurveData, at: usize, classes: &[usize]) -> Option<usize> {
        let ends = match data {
            CurveData::BSpline { basis, ctrl } if basis.is_clamped() => {
                [ctrl[0], ctrl[ctrl.len() - 1]]
            }
            CurveData::Fitted { fit, ends, .. } if ends[0] != SplineEnd::Periodic => {
                [fit[0], fit[fit.len() - 1]]
            }
            _ => return None,
        };
        (0..2).find(|&k| classes[ends[k]] == classes[at])
    }

    /// The characteristic length of a line or curve (at least 1 mm): a
    /// line's length, a radius, a spline's control polygon.
    fn size_of(&self, s: Size) -> f64 {
        let len = match s {
            Size::Line(l) => match self.lines[l] {
                Some([a, b]) => dist(self.xyz(a), self.xyz(b)),
                None => 1.0,
            },
            Size::Curve(c) => match &self.curves[c] {
                Some(CurveData::Arc { c, s, .. }) => dist(self.xyz(*c), self.xyz(*s)),
                Some(CurveData::Circle { r, .. }) => self.params[*r],
                Some(d) if d.is_spline() => self
                    .ctrl_columns(d)
                    .windows(2)
                    .map(|w| dist(self.col3(w[0]), self.col3(w[1])))
                    .sum::<f64>(),
                Some(CurveData::Reference(rc)) => rc.length(),
                Some(CurveData::Ellipse { c, a, b, .. }) => {
                    dist(self.xyz(*c), self.xyz(*a)).max(self.params[*b])
                }
                _ => 1.0,
            },
        };
        if len.is_finite() { len.max(1.0) } else { 1.0 }
    }

    fn size(c: SpatialCurve) -> Size {
        match c {
            SpatialCurve::Line(l) => Size::Line(l.index()),
            SpatialCurve::Curve(c) => Size::Curve(c.index()),
        }
    }

    // Constraints.

    /// A point on a round: an arc's end (by identity or coincidence) or a
    /// point placed on it.
    fn on_round(&self, curve: SpatialCurveId, at: usize, classes: &[usize]) -> bool {
        if let CurveData::Arc { s, e, .. } = *self.data(curve)
            && (classes[s] == classes[at] || classes[e] == classes[at])
        {
            return true;
        }
        self.live_constraints().any(|(_, r)| match r.def {
            SpatialConstraint::PointOn { point, curve: on } => {
                on == SpatialCurve::Curve(curve) && classes[point.index()] == classes[at]
            }
            _ => false,
        })
    }

    fn check_definition(&self, c: &SpatialConstraint) -> Result<(), SpatialError> {
        use SpatialConstraint::*;
        let invalid = |what| Err(SpatialError::InvalidValue(what));
        for p in c.points() {
            self.pidx(p)?;
        }
        for curve in c.curves() {
            match curve {
                SpatialCurve::Line(l) => {
                    self.lidx(l)?;
                }
                SpatialCurve::Curve(id) => {
                    self.cidx(id)?;
                }
            }
        }
        let kind = |c: SpatialCurve| match c {
            SpatialCurve::Line(_) => None,
            SpatialCurve::Curve(id) => Some(self.data(id).kind()),
        };
        let round = |c: SpatialCurve| {
            matches!(
                kind(c),
                Some(SpatialEntityKind::Arc | SpatialEntityKind::Circle)
            )
        };
        let defines = |p: SpatialPointId, c: SpatialCurve| match c {
            SpatialCurve::Line(l) => self.lines[l.index()].is_some_and(|e| e.contains(&p.index())),
            SpatialCurve::Curve(id) => self.data(id).points().contains(&p.index()),
        };
        let positive = |v: f64| v.is_finite() && v > SPATIAL_MIN_SEGMENT;
        match *c {
            Coincident(a, b) | Offset { a, b, .. } if a == b => {
                invalid("a relation between a point and itself")
            }
            Length { value, .. } if !positive(value) => {
                invalid("a length must be finite and longer than the shortest line")
            }
            Fix { at: t, .. } | Attach { target: t, .. } if !finite3(t) => {
                invalid("a value is not finite")
            }
            Offset { value, .. } if !value.is_finite() => invalid("a value is not finite"),
            OnSurface { surface, .. } => surface.check().or_else(invalid),
            PointOn { point, curve } | Midpoint { point, curve } if defines(point, curve) => {
                invalid("a point on a curve it defines")
            }
            Midpoint { curve, .. } => match curve {
                SpatialCurve::Line(_) => Ok(()),
                SpatialCurve::Curve(id) => match self.data(id) {
                    CurveData::Arc { .. } => Ok(()),
                    CurveData::Reference(rc) if rc.midpoint().is_some() => Ok(()),
                    _ => invalid("a midpoint of a line, an arc or an open reference curve"),
                },
            },
            Tangent { a, b, at } => {
                let classes = self.classes();
                if a == b {
                    return invalid("a tangency between two curves");
                }
                for c in [a, b] {
                    let ok = match c {
                        SpatialCurve::Line(_) => true,
                        SpatialCurve::Curve(id) => {
                            let data = self.data(id);
                            if data.is_round() {
                                self.on_round(id, at.index(), &classes)
                            } else {
                                self.spline_end(data, at.index(), &classes).is_some()
                            }
                        }
                    };
                    if !ok {
                        return invalid(
                            "a tangency at a point of each arc or circle and at an end of each \
                             open spline",
                        );
                    }
                }
                Ok(())
            }
            Equal(a, b) => {
                let lines = matches!((a, b), (SpatialCurve::Line(_), SpatialCurve::Line(_)));
                if a == b || !(lines || (round(a) && round(b))) {
                    invalid("equal lengths of two lines or radii of two arcs or circles")
                } else {
                    Ok(())
                }
            }
            Radius { curve, value } | Diameter { curve, value } => {
                if !round(SpatialCurve::Curve(curve)) {
                    invalid("a radius or diameter of an arc or circle")
                } else if !positive(value) {
                    invalid("a radius must be finite and longer than the shortest line")
                } else {
                    Ok(())
                }
            }
            ArcLength { curve, value } => {
                if kind(SpatialCurve::Curve(curve)) != Some(SpatialEntityKind::Arc) {
                    invalid("an arc length of an arc")
                } else if !positive(value) {
                    invalid("an arc length must be finite and longer than the shortest line")
                } else {
                    Ok(())
                }
            }
            NormalAlign { curve, .. } => match *self.data(curve) {
                CurveData::Arc {
                    normal: NormalRec::Free(_),
                    ..
                }
                | CurveData::Circle {
                    normal: NormalRec::Free(_),
                    ..
                } => Ok(()),
                _ => invalid("a normal alignment of an arc or circle with a free normal"),
            },
            EllipseRadius { curve, value, .. } => {
                if kind(SpatialCurve::Curve(curve)) != Some(SpatialEntityKind::Ellipse) {
                    invalid("a major or minor radius of an ellipse")
                } else if !positive(value) {
                    invalid("a radius must be finite and longer than the shortest line")
                } else {
                    Ok(())
                }
            }
            Along(along) => {
                let d = along.dir();
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if !finite3(d) || (len - 1.0).abs() > 1e-9 {
                    return invalid("a relation along a direction takes a unit direction");
                }
                match along {
                    SpatialAlong::Offset { a, b, value, .. } => {
                        if a == b {
                            invalid("a relation between a point and itself")
                        } else if !value.is_finite() {
                            invalid("a value is not finite")
                        } else {
                            Ok(())
                        }
                    }
                    SpatialAlong::Line { .. } => Ok(()),
                    SpatialAlong::Normal { curve, .. } => match *self.data(curve) {
                        CurveData::Arc {
                            normal: NormalRec::Free(_),
                            ..
                        }
                        | CurveData::Circle {
                            normal: NormalRec::Free(_),
                            ..
                        } => Ok(()),
                        _ => invalid(
                            "a normal along a direction of an arc or circle with a free normal",
                        ),
                    },
                }
            }
            InPlane(_) => invalid("in-plane relations are added with add_in_plane"),
            Parallel(a, b) | Perpendicular(a, b) | Angle { a, b, .. } => {
                if a == b {
                    return invalid("a relation between two directions");
                }
                for d in [a, b] {
                    if let SpatialDir::Normal(id) = d
                        && !self.data(id).is_round()
                    {
                        return invalid("a direction of a line or the normal of an arc or circle");
                    }
                }
                if self.dir_fixed(a) && self.dir_fixed(b) {
                    return invalid("a relation between two directions bound to frame axes");
                }
                match *c {
                    Angle { value, .. }
                        if !value.is_finite() || value <= 0.0 || value >= std::f64::consts::PI =>
                    {
                        invalid("an angle strictly between nothing and a half turn")
                    }
                    _ => Ok(()),
                }
            }
            Symmetric { a, b, plane } => {
                if a == b {
                    return invalid("a relation between a point and itself");
                }
                match plane {
                    SpatialSurface::Plane { .. } => plane.check().or_else(invalid),
                    _ => invalid("a symmetry about a plane"),
                }
            }
            Concentric { a, b, normal } => {
                if a == b || !round(SpatialCurve::Curve(a)) || !round(SpatialCurve::Curve(b)) {
                    return invalid("concentric takes two arcs or circles");
                }
                match (self.round_normal(a), self.round_normal(b)) {
                    (NormalRec::Axis(x, _), NormalRec::Axis(y, _)) if normal && x != y => {
                        invalid("concentric arcs or circles bound to planes square to two axes")
                    }
                    _ => Ok(()),
                }
            }
            Curvature { a, b, at } => {
                let spline = |c: SpatialCurve| match c {
                    SpatialCurve::Curve(id) => self.data(id).is_spline(),
                    SpatialCurve::Line(_) => false,
                };
                if !spline(a) && !spline(b) {
                    return invalid("curvature continuity at an end of a spline");
                }
                if kind(a) == Some(SpatialEntityKind::Reference)
                    || kind(b) == Some(SpatialEntityKind::Reference)
                {
                    return invalid("curvature continuity of lines, arcs and splines");
                }
                self.check_definition(&Tangent { a, b, at })
            }
            SplineLength { curve, value } => {
                if !self.data(curve).is_spline() {
                    invalid("a spline length of a spline")
                } else if !positive(value) {
                    invalid("a length must be finite and longer than the shortest line")
                } else {
                    Ok(())
                }
            }
            TangentLength { curve, end, value } => {
                let data = self.data(curve);
                let ends = match data {
                    CurveData::BSpline { basis, .. } => basis.is_clamped(),
                    CurveData::Fitted { ends, .. } => ends[0] != SplineEnd::Periodic,
                    _ => false,
                };
                if !ends || end > 1 {
                    invalid("a tangent magnitude at the start or end of an open spline")
                } else if !positive(value) {
                    invalid("a tangent magnitude must be finite and greater than nothing")
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    /// Whether a direction is constant: the normal of an arc or circle
    /// bound to a frame axis.
    fn dir_fixed(&self, d: SpatialDir) -> bool {
        match d {
            SpatialDir::Line(_) => false,
            SpatialDir::Normal(c) => matches!(self.round_normal(c), NormalRec::Axis(..)),
        }
    }

    /// The normal of an arc or circle (validated as round).
    fn round_normal(&self, c: SpatialCurveId) -> NormalRec {
        match *self.data(c) {
            CurveData::Arc { normal, .. } | CurveData::Circle { normal, .. } => normal,
            _ => unreachable!("validated as round"),
        }
    }

    /// Adds a constraint or dimension.
    pub fn add_constraint(
        &mut self,
        c: SpatialConstraint,
    ) -> Result<SpatialConstraintId, SpatialError> {
        self.check_definition(&c)?;
        let mut params = Vec::new();
        let mut sense = 1.0;
        match c {
            SpatialConstraint::PointOn { point, curve } => {
                let p = self.xyz(point.index());
                let param = match curve {
                    SpatialCurve::Line(l) => {
                        let [a, b] = self.lines[l.index()].expect("checked");
                        let (pa, d) = (self.xyz(a), self.delta(a, b));
                        let dd: f64 = d.iter().map(|x| x * x).sum();
                        let t = if dd > 0.0 {
                            (0..3).map(|k| (p[k] - pa[k]) * d[k]).sum::<f64>() / dd
                        } else {
                            0.0
                        };
                        Some((t.clamp(0.0, 1.0), Some((0.0, 1.0))))
                    }
                    SpatialCurve::Curve(id) => match self.data(id) {
                        CurveData::Reference(rc) => {
                            let ((lo, hi), bounded) = rc.domain();
                            Some((rc.closest(p), bounded.then_some((lo, hi))))
                        }
                        d if d.is_spline() => {
                            let (lo, hi) = self.spline_basis(d).domain();
                            let t = curve::closest(|t| self.spline_at(d, t), lo, hi, p);
                            Some((t, Some((lo, hi))))
                        }
                        _ => None,
                    },
                };
                if let Some((t, bounds)) = param {
                    params.push(self.new_columns(&[t], 1.0, bounds));
                }
            }
            SpatialConstraint::Tangent { a, b, at } | SpatialConstraint::Curvature { a, b, at } => {
                let classes = self.classes();
                if let (Some(u), Some(v)) = (
                    self.tan_value(a, at.index(), &classes),
                    self.tan_value(b, at.index(), &classes),
                ) && u[0] * v[0] + u[1] * v[1] + u[2] * v[2] < 0.0
                {
                    sense = -1.0;
                }
            }
            _ => {}
        }
        let last = self.measure_def(&c).into_iter().collect::<Vec<f64>>();
        let last = match c {
            SpatialConstraint::Fix { point, .. } | SpatialConstraint::Attach { point, .. } => {
                self.point(point).unwrap_or([0.0; 3]).to_vec()
            }
            _ => last,
        };
        self.constraints.push(Some(ConstraintRec {
            def: c,
            last,
            params,
            sense,
        }));
        self.invalidate();
        Ok(SpatialConstraintId(self.constraints.len() as u32 - 1))
    }

    /// Removes a constraint or dimension.
    pub fn remove_constraint(&mut self, id: SpatialConstraintId) -> Result<(), SpatialError> {
        if let SpatialConstraint::InPlane(c) = self.rec(id)?.def {
            self.plane.remove(c);
        }
        self.constraints[id.index()] = None;
        self.invalidate();
        Ok(())
    }

    pub fn constraint(&self, id: SpatialConstraintId) -> Option<&SpatialConstraint> {
        self.rec(id).ok().map(|r| &r.def)
    }

    /// All constraints in creation order.
    pub fn constraints(&self) -> impl Iterator<Item = (SpatialConstraintId, &SpatialConstraint)> {
        self.live_constraints()
            .map(|(i, c)| (SpatialConstraintId(i as u32), &c.def))
    }

    /// The values the caller may change ([`SpatialSystem::set_value`],
    /// [`SpatialSystem::set_target`]).
    fn values(&self, def: &SpatialConstraint) -> Vec<f64> {
        use SpatialConstraint::*;
        match *def {
            Fix { at: t, .. } | Attach { target: t, .. } => t.to_vec(),
            Offset { value, .. }
            | Length { value, .. }
            | Radius { value, .. }
            | Diameter { value, .. }
            | EllipseRadius { value, .. }
            | Angle { value, .. }
            | SplineLength { value, .. }
            | TangentLength { value, .. }
            | Along(SpatialAlong::Offset { value, .. })
            | ArcLength { value, .. } => vec![value],
            InPlane(c) => self
                .plane
                .sys
                .constraint(c)
                .and_then(Constraint::value)
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Changes the value of a `Length`, `Offset`, `Radius`, `Diameter`,
    /// `ArcLength` or in-plane dimension. The structure (and the cached
    /// rows' layout) stays; the next solve applies it.
    pub fn set_value(&mut self, id: SpatialConstraintId, value: f64) -> Result<(), SpatialError> {
        let mut def = self.rec(id)?.def;
        match &mut def {
            SpatialConstraint::Length { value: v, .. }
            | SpatialConstraint::Offset { value: v, .. }
            | SpatialConstraint::Radius { value: v, .. }
            | SpatialConstraint::Diameter { value: v, .. }
            | SpatialConstraint::EllipseRadius { value: v, .. }
            | SpatialConstraint::Angle { value: v, .. }
            | SpatialConstraint::SplineLength { value: v, .. }
            | SpatialConstraint::TangentLength { value: v, .. }
            | SpatialConstraint::Along(SpatialAlong::Offset { value: v, .. })
            | SpatialConstraint::ArcLength { value: v, .. } => *v = value,
            SpatialConstraint::InPlane(c) => {
                self.plane
                    .sys
                    .set_dimension_value(*c, value)
                    .map_err(SpatialError::InPlane)?;
                if let Some(p) = &mut self.prepared {
                    p.update_value(id.index(), 0, value);
                }
                return Ok(());
            }
            _ => {
                return Err(SpatialError::WrongConstraintKind {
                    constraint: id,
                    expected: "a dimension",
                });
            }
        }
        self.check_definition(&def)?;
        self.constraints[id.index()]
            .as_mut()
            .expect("checked above")
            .def = def;
        if let Some(p) = &mut self.prepared {
            p.update_value(id.index(), 0, value);
        }
        Ok(())
    }

    /// Changes the target of a `Fix` or `Attach` (a moved source vertex,
    /// mapped into the local frame).
    pub fn set_target(
        &mut self,
        id: SpatialConstraintId,
        target: [f64; 3],
    ) -> Result<(), SpatialError> {
        let mut def = self.rec(id)?.def;
        match &mut def {
            SpatialConstraint::Fix { at: t, .. } | SpatialConstraint::Attach { target: t, .. } => {
                *t = target
            }
            _ => {
                return Err(SpatialError::WrongConstraintKind {
                    constraint: id,
                    expected: "a fix or an attachment",
                });
            }
        }
        self.check_definition(&def)?;
        self.constraints[id.index()]
            .as_mut()
            .expect("checked above")
            .def = def;
        if let Some(p) = &mut self.prepared {
            for (k, &v) in target.iter().enumerate() {
                p.update_value(id.index(), k, v);
            }
        }
        Ok(())
    }

    /// Changes the surface of an `OnSurface` (a moved face, or the next
    /// touching plane of a face solved through the kernel's projection).
    pub fn set_surface(
        &mut self,
        id: SpatialConstraintId,
        surface: SpatialSurface,
    ) -> Result<(), SpatialError> {
        let mut def = self.rec(id)?.def;
        match &mut def {
            SpatialConstraint::OnSurface { surface: s, .. } => *s = surface,
            _ => {
                return Err(SpatialError::WrongConstraintKind {
                    constraint: id,
                    expected: "a point on a surface",
                });
            }
        }
        self.check_definition(&def)?;
        self.constraints[id.index()]
            .as_mut()
            .expect("checked above")
            .def = def;
        // The row carries the surface: it is made again.
        self.invalidate();
        Ok(())
    }

    /// True length of a line (a driven measurement: adds no row).
    pub fn line_length(&self, l: SpatialLineId) -> Option<f64> {
        let [a, b] = self.lidx(l).ok()?;
        let d = self.delta(a, b);
        Some((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt())
    }

    /// Signed difference `b - a` along an axis (a driven measurement).
    pub fn offset(&self, a: SpatialPointId, b: SpatialPointId, axis: Axis) -> Option<f64> {
        let (a, b) = (self.pidx(a).ok()?, self.pidx(b).ok()?);
        Some(self.delta(a, b)[axis.index()])
    }

    fn measure_def(&self, def: &SpatialConstraint) -> Option<f64> {
        match *def {
            SpatialConstraint::Length { line, .. } => self.line_length(line),
            SpatialConstraint::Offset { a, b, axis, .. } => self.offset(a, b, axis),
            SpatialConstraint::Radius { curve, .. } => self.radius(curve),
            SpatialConstraint::Diameter { curve, .. } => self.radius(curve).map(|r| 2.0 * r),
            SpatialConstraint::ArcLength { curve, .. } => self.arc_length(curve),
            SpatialConstraint::EllipseRadius { curve, minor, .. } => self
                .ellipse_radii(curve)
                .map(|(major, b)| if minor { b } else { major }),
            SpatialConstraint::InPlane(c) => self.plane.measure(c),
            SpatialConstraint::Angle { a, b, .. } => self.angle(a, b),
            SpatialConstraint::SplineLength { curve, .. } => self.spline_length(curve),
            SpatialConstraint::TangentLength { curve, end, .. } => self.tangent_length(curve, end),
            SpatialConstraint::Along(SpatialAlong::Offset { a, b, dir, .. }) => {
                self.offset_along(a, b, dir)
            }
            _ => None,
        }
    }

    /// Signed difference `(b - a).dir` along a unit direction (a driven
    /// measurement, mitcad#1661).
    pub fn offset_along(&self, a: SpatialPointId, b: SpatialPointId, dir: [f64; 3]) -> Option<f64> {
        let (a, b) = (self.pidx(a).ok()?, self.pidx(b).ok()?);
        let d = self.delta(a, b);
        Some(d[0] * dir[0] + d[1] * dir[1] + d[2] * dir[2])
    }

    /// The unit vector of a direction now (mitcad#1661).
    pub fn direction(&self, d: SpatialDir) -> Option<[f64; 3]> {
        match d {
            SpatialDir::Line(l) => {
                let [a, b] = self.lidx(l).ok()?;
                let v = self.delta(a, b);
                let len = curve::norm(v);
                (len > 0.0).then(|| v.map(|x| x / len))
            }
            SpatialDir::Normal(c) => self.normal(c),
        }
    }

    /// The angle between two directions (radians, in `[0, pi]`; a driven
    /// measurement, mitcad#1661).
    pub fn angle(&self, a: SpatialDir, b: SpatialDir) -> Option<f64> {
        let (u, v) = (self.direction(a)?, self.direction(b)?);
        let (u, v): (V3<f64>, V3<f64>) = (V3::cst(u), V3::cst(v));
        Some(u.cross(v).norm().atan2(u.dot(v)))
    }

    /// The magnitude of an open spline's end tangent `abs(C'(t))` at its
    /// first (`end` 0) or last (`end` 1) point (a driven measurement,
    /// mitcad#1661).
    pub fn tangent_length(&self, c: SpatialCurveId, end: usize) -> Option<f64> {
        let data = self.cidx(c).ok()?;
        if !data.is_spline() || end > 1 {
            return None;
        }
        let basis = self.spline_basis(data);
        let (lo, hi) = basis.domain();
        let cp: Vec<V3<f64>> = self
            .ctrl_columns(data)
            .iter()
            .map(|&col| V3::cst(self.col3(col)))
            .collect();
        let t = if end == 0 { lo } else { hi };
        Some(basis.eval(&cp, t, 1).d1.norm())
    }

    /// The current geometric value of a dimension, measured as it is
    /// constrained (in-plane ones as of the last solve).
    pub fn measure(&self, id: SpatialConstraintId) -> Option<f64> {
        self.measure_def(&self.rec(id).ok()?.def)
    }

    /// The sense of a tangency: whether its curves' tangents point the same
    /// way, as taken from the geometry when it was added or as set
    /// ([`SpatialSystem::set_tangent_sense`]). None for another relation.
    pub fn tangent_sense(&self, id: SpatialConstraintId) -> Option<AxisSense> {
        let rec = self.rec(id).ok()?;
        match rec.def {
            SpatialConstraint::Tangent { .. } | SpatialConstraint::Curvature { .. }
                if rec.sense < 0.0 =>
            {
                Some(AxisSense::Negative)
            }
            SpatialConstraint::Tangent { .. } | SpatialConstraint::Curvature { .. } => {
                Some(AxisSense::Positive)
            }
            _ => None,
        }
    }

    /// Sets the sense a tangency keeps (a stored definition's), in place of
    /// the one taken from the geometry when it was added; the solve
    /// validates it ([`SpatialIssue::ReversedTangent`]).
    pub fn set_tangent_sense(
        &mut self,
        id: SpatialConstraintId,
        sense: AxisSense,
    ) -> Result<(), SpatialError> {
        if !matches!(
            self.rec(id)?.def,
            SpatialConstraint::Tangent { .. } | SpatialConstraint::Curvature { .. }
        ) {
            return Err(SpatialError::WrongConstraintKind {
                constraint: id,
                expected: "a tangency",
            });
        }
        if let Some(Some(rec)) = self.constraints.get_mut(id.index()) {
            rec.sense = sense.sign();
        }
        Ok(())
    }

    // In-plane relations of converted sketches and of plane-bound shapes
    // (mitcad#1550: on the plane square to any frame axis).

    fn shadow_point(&mut self, p: usize, k: usize) -> PointId {
        let col = self.pc(p);
        self.plane.point(p, k, col, &self.params)
    }

    /// The planar point that stands for a spatial point in in-plane
    /// relations ([`SpatialSystem::add_in_plane`]) of the plane square to
    /// `axis`: its two coordinates in that plane (the sketch plane's x and
    /// y for Z, y and z for X, z and x for Y).
    pub fn in_plane_point(
        &mut self,
        p: SpatialPointId,
        axis: Axis,
    ) -> Result<PointId, SpatialError> {
        let i = self.pidx(p)?;
        Ok(self.shadow_point(i, axis.index()))
    }

    /// The planar entity that stands for a line, an arc whose normal is
    /// `axis` (positive), a circle about `axis` (either way), a full
    /// ellipse in the plane square to `axis`, or (in the sketch plane, Z
    /// only) a spline (its x and y; a fit-point spline with the same basis
    /// and hidden control points) in in-plane relations of the plane square
    /// to `axis`.
    pub fn in_plane_curve(
        &mut self,
        c: SpatialCurve,
        axis: Axis,
    ) -> Result<EntityId, SpatialError> {
        self.in_plane_curve_k(c, axis.index())
    }

    /// A rotated plane for in-plane relations (mitcad#1661): the plane
    /// spanned by the unit, square in-plane axes `x` and `y` (its normal `x
    /// x y`), wherever along its normal (the shape's own relations place
    /// it). Its stand-ins' planar x and y are the points' coordinates
    /// along `x` and `y`. Returns the plane's key for
    /// [`SpatialSystem::in_plane_point_in`] and
    /// [`SpatialSystem::in_plane_curve_in`]; the same axes give the same
    /// key.
    pub fn add_in_plane_frame(&mut self, x: [f64; 3], y: [f64; 3]) -> Result<usize, SpatialError> {
        let unit =
            |v: [f64; 3]| ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() - 1.0).abs() <= 1e-9;
        let square = (x[0] * y[0] + x[1] * y[1] + x[2] * y[2]).abs() <= 1e-9;
        if !finite3(x) || !finite3(y) || !unit(x) || !unit(y) || !square {
            return Err(SpatialError::InvalidValue(
                "a rotated plane takes two unit in-plane axes at right angles",
            ));
        }
        Ok(self.plane.add_frame(x, y))
    }

    /// [`SpatialSystem::in_plane_point`] in the plane of a key: a frame
    /// axis' index (0, 1, 2) or a rotated plane's
    /// ([`SpatialSystem::add_in_plane_frame`]).
    pub fn in_plane_point_in(
        &mut self,
        p: SpatialPointId,
        plane: usize,
    ) -> Result<PointId, SpatialError> {
        let i = self.pidx(p)?;
        if plane >= self.plane.planes() {
            return Err(SpatialError::InvalidValue("an unknown in-plane plane"));
        }
        Ok(self.shadow_point(i, plane))
    }

    /// [`SpatialSystem::in_plane_curve`] in the plane of a key; a rotated
    /// plane's take lines, circles with a free normal (held square to it
    /// by [`SpatialAlong::Normal`]) and full ellipses in it.
    pub fn in_plane_curve_in(
        &mut self,
        c: SpatialCurve,
        plane: usize,
    ) -> Result<EntityId, SpatialError> {
        if plane >= self.plane.planes() {
            return Err(SpatialError::InvalidValue("an unknown in-plane plane"));
        }
        self.in_plane_curve_k(c, plane)
    }

    fn in_plane_curve_k(&mut self, c: SpatialCurve, k: usize) -> Result<EntityId, SpatialError> {
        match c {
            SpatialCurve::Line(l) => {
                let [a, b] = self.lidx(l)?;
                if let Some(e) = self.plane.line_entity(l.index(), k) {
                    return Ok(e);
                }
                let (pa, pb) = (self.shadow_point(a, k), self.shadow_point(b, k));
                let e = self
                    .plane
                    .sys
                    .add_line(pa, pb)
                    .map_err(SpatialError::InPlane)?;
                self.plane.set_line(l.index(), k, e);
                Ok(e)
            }
            SpatialCurve::Curve(id) => {
                let data = self.cidx(id)?.clone();
                if let Some(e) = self.plane.curve_entity(id.index(), k) {
                    return Ok(e);
                }
                let (e, cols) = match &data {
                    CurveData::Arc {
                        c,
                        s,
                        e,
                        normal: NormalRec::Axis(n, AxisSense::Positive),
                    } if n.index() == k => {
                        let (pc, ps, pe) = (
                            self.shadow_point(*c, k),
                            self.shadow_point(*s, k),
                            self.shadow_point(*e, k),
                        );
                        (self.plane.sys.add_arc(pc, ps, pe), Vec::new())
                    }
                    CurveData::Circle {
                        c,
                        r,
                        normal: NormalRec::Axis(n, _),
                    } if n.index() == k => {
                        let pc = self.shadow_point(*c, k);
                        let radius = self.params[*r];
                        (self.plane.sys.add_circle(pc, radius), vec![*r])
                    }
                    // A circle of a rotated plane (mitcad#1661): its free
                    // normal is held square to the plane by its own
                    // relation.
                    CurveData::Circle {
                        c,
                        r,
                        normal: NormalRec::Free(_),
                    } if k >= 3 => {
                        let pc = self.shadow_point(*c, k);
                        let radius = self.params[*r];
                        (self.plane.sys.add_circle(pc, radius), vec![*r])
                    }
                    CurveData::Ellipse {
                        c,
                        a,
                        b,
                        axis: n,
                        ends: None,
                        dir,
                    } if (dir.is_none() && n.index() == k) || (dir.is_some() && k >= 3) => {
                        let (pc, pa) = (self.shadow_point(*c, k), self.shadow_point(*a, k));
                        let minor = self.params[*b];
                        (self.plane.sys.add_ellipse(pc, pa, minor), vec![*b])
                    }
                    CurveData::BSpline { basis, ctrl } if k == 2 => {
                        let pts: Vec<PointId> =
                            ctrl.iter().map(|&p| self.shadow_point(p, k)).collect();
                        (
                            self.plane.sys.add_bspline(
                                basis.degree,
                                &pts,
                                Some(basis.weights.as_slice()),
                                Some(basis.knots.as_slice()),
                            ),
                            Vec::new(),
                        )
                    }
                    CurveData::Fitted {
                        basis,
                        fit,
                        hidden,
                        params,
                        ends,
                    } if k == 2 => {
                        let pts: Vec<PointId> =
                            fit.iter().map(|&p| self.shadow_point(p, k)).collect();
                        let xy: Vec<[f64; 2]> = hidden
                            .iter()
                            .map(|&h| [self.params[h], self.params[h + 1]])
                            .collect();
                        (
                            self.plane.sys.add_fitted_with(
                                &pts,
                                *ends,
                                basis.clone(),
                                params.clone(),
                                &xy,
                            ),
                            hidden.clone(),
                        )
                    }
                    _ => {
                        return Err(SpatialError::InvalidValue(
                            "in-plane relations take lines, and arcs, circles and full \
                             ellipses in their plane (splines in the sketch plane)",
                        ));
                    }
                };
                let e = e.map_err(SpatialError::InPlane)?;
                self.plane.set_curve(id.index(), k, e, &cols);
                Ok(e)
            }
        }
    }

    /// Adds a planar constraint or dimension of a converted sketch over the
    /// planar points and entities of [`SpatialSystem::in_plane_point`] and
    /// [`SpatialSystem::in_plane_curve`]: the planar solver's own rows,
    /// solved on the x and y columns of the spatial points (its
    /// orientation choices taken from the current x and y, as in the planar
    /// sketch).
    pub fn add_in_plane(&mut self, c: Constraint) -> Result<SpatialConstraintId, SpatialError> {
        self.plane.sync(&self.params);
        let index = self.constraints.len();
        let (id, owned) = self.plane.add(c, index).map_err(SpatialError::InPlane)?;
        let mut params = Vec::new();
        for g in owned {
            let bounds = match self.plane.sys.pkind[g] {
                ParamKind::SplineT { lo, hi } => Some((lo, hi)),
                _ => None,
            };
            let (v, w) = (self.plane.sys.params[g], self.plane.sys.weights[g]);
            let col = self.new_columns(&[v], w, bounds);
            self.plane.map_param(g, col);
            params.push(col);
        }
        let def = SpatialConstraint::InPlane(id);
        let last = self.values(&def);
        self.constraints.push(Some(ConstraintRec {
            def,
            last,
            params,
            sense: 1.0,
        }));
        self.invalidate();
        Ok(SpatialConstraintId(index as u32))
    }

    /// The planar definition of an in-plane relation.
    pub fn in_plane_constraint(&self, id: SpatialConstraintId) -> Option<&Constraint> {
        match self.rec(id).ok()?.def {
            SpatialConstraint::InPlane(c) => self.plane.sys.constraint(c),
            _ => None,
        }
    }

    /// The points of `points` that carry one [`SpatialConstraint::OnSketchPlane`]
    /// each when a planar sketch is converted: the lowest point id of each
    /// group whose elevations are tied (by coincidences and plane-bound
    /// arcs' centre, start and end), so that each new `z` is determined
    /// exactly once and the planar sketch's DOF and redundancy stay.
    pub fn elevation_groups(&self, points: &[SpatialPointId]) -> Vec<SpatialPointId> {
        let mut parent: Vec<usize> = (0..self.points.len()).collect();
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let join = |parent: &mut [usize], a: usize, b: usize| {
            let (ra, rb) = (find(parent, a), find(parent, b));
            if ra != rb {
                parent[ra.max(rb)] = ra.min(rb);
            }
        };
        for (_, rec) in self.live_constraints() {
            if let SpatialConstraint::Coincident(a, b) = rec.def {
                join(&mut parent[..], a.index(), b.index());
            }
        }
        for (_, data) in self.live_curves() {
            match *data {
                CurveData::Arc {
                    c,
                    s,
                    e,
                    normal: NormalRec::Axis(..),
                } => {
                    join(&mut parent[..], c, s);
                    join(&mut parent[..], c, e);
                }
                // A plane-bound ellipse in a plane parallel to the sketch
                // plane: its points share the centre's elevation.
                CurveData::Ellipse {
                    axis: Axis::Z,
                    dir: None,
                    ref ends,
                    c,
                    a,
                    ..
                } => {
                    join(&mut parent[..], c, a);
                    for &p in ends.iter().flatten() {
                        join(&mut parent[..], c, p);
                    }
                }
                _ => {}
            }
        }
        let mut out: Vec<SpatialPointId> = points
            .iter()
            .filter(|p| self.pidx(**p).is_ok())
            .map(|p| SpatialPointId(find(&mut parent, p.index()) as u32))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Union-find classes of points joined by coincidences.
    fn classes(&self) -> Vec<usize> {
        let mut parent: Vec<usize> = (0..self.points.len()).collect();
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for (_, c) in self.live_constraints() {
            if let SpatialConstraint::Coincident(a, b) = c.def {
                let (ra, rb) = (find(&mut parent, a.index()), find(&mut parent, b.index()));
                if ra != rb {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
        (0..parent.len()).map(|i| find(&mut parent, i)).collect()
    }

    /// Brings the in-plane shadow up to date (values and mirrored
    /// coincidences).
    fn sync_plane(&mut self) {
        if self.plane.is_empty() {
            return;
        }
        self.plane.sync(&self.params);
        let pairs: Vec<(usize, usize)> = self
            .live_constraints()
            .filter_map(|(_, r)| match r.def {
                SpatialConstraint::Coincident(a, b) => Some((a.index(), b.index())),
                _ => None,
            })
            .collect();
        self.plane.mirror(&pairs);
    }

    /// What the definition alone shows, before any solve: lines and curves
    /// whose defining points are one point (by identity or coincidences),
    /// and contradictory requests: on the same pair of points, alignments
    /// with two axes or two senses (only a collapsed line satisfies both),
    /// or an offset along the aligned axis that points against the sense or
    /// is zero; normal alignments of one curve with two axes or senses; and
    /// the planar direction graph's contradictions of in-plane relations.
    fn check_structure(&mut self) -> (Vec<SpatialDependency>, Vec<SpatialIssue>) {
        self.sync_plane();
        let classes = self.classes();
        let mut issues = Vec::new();
        let mut collapsed = vec![false; self.lines.len()];
        for (i, [a, b]) in self.live_lines() {
            if classes[a] == classes[b] {
                collapsed[i] = true;
                issues.push(SpatialIssue::CollapsedLine(SpatialLineId(i as u32)));
            }
        }
        for (i, data) in self.live_curves() {
            let joined = |a: usize, b: usize| classes[a] == classes[b];
            let bad = match data {
                CurveData::Arc { c, s, e, .. } => {
                    joined(*c, *s) || joined(*c, *e) || joined(*s, *e)
                }
                CurveData::Fitted { fit, ends, .. } => {
                    fit.windows(2).any(|w| joined(w[0], w[1]))
                        || (ends[0] == SplineEnd::Periodic && joined(fit[0], fit[fit.len() - 1]))
                }
                CurveData::BSpline { ctrl, .. } => ctrl.iter().all(|&p| joined(p, ctrl[0])),
                CurveData::Ellipse { c, a, ends, .. } => {
                    joined(*c, *a) || ends.is_some_and(|[s, e]| joined(s, e))
                }
                _ => false,
            };
            if bad {
                issues.push(SpatialIssue::DegenerateCurve(SpatialCurveId(i as u32)));
            }
        }
        // By pair of classes (lower first): alignments (axis, whether they
        // point positive as seen from the lower class, constraint) and
        // offsets (axis, value as seen from the lower class, constraint).
        type Pair = (usize, usize);
        let pair = |a: usize, b: usize| -> (Pair, bool) {
            let (ca, cb) = (classes[a], classes[b]);
            if ca < cb {
                ((ca, cb), true)
            } else {
                ((cb, ca), false)
            }
        };
        let mut aligns: BTreeMap<Pair, Vec<(Axis, bool, usize)>> = BTreeMap::new();
        let mut offsets: BTreeMap<Pair, Vec<(Axis, f64, usize)>> = BTreeMap::new();
        let mut normals: BTreeMap<usize, Vec<(Axis, AxisSense, usize)>> = BTreeMap::new();
        for (i, rec) in self.live_constraints() {
            match rec.def {
                SpatialConstraint::Align { line, axis, sense } => {
                    if collapsed[line.index()] {
                        continue;
                    }
                    let [a, b] = self.lines[line.index()].expect("live line");
                    let (key, forward) = pair(a, b);
                    let positive = forward == (sense == AxisSense::Positive);
                    aligns.entry(key).or_default().push((axis, positive, i));
                }
                SpatialConstraint::Offset { a, b, axis, value } => {
                    let (a, b) = (a.index(), b.index());
                    if classes[a] == classes[b] {
                        continue;
                    }
                    let (key, forward) = pair(a, b);
                    let value = if forward { value } else { -value };
                    offsets.entry(key).or_default().push((axis, value, i));
                }
                SpatialConstraint::NormalAlign { curve, axis, sense } => {
                    normals
                        .entry(curve.index())
                        .or_default()
                        .push((axis, sense, i));
                }
                _ => {}
            }
        }
        let mut conflicts = Vec::new();
        for (key, list) in &aligns {
            let (axis, positive, _) = list[0];
            if list.iter().any(|&(x, p, _)| x != axis || p != positive) {
                conflicts.push(dependency(list.iter().map(|a| a.2).collect()));
                continue;
            }
            let sign = if positive { 1.0 } else { -1.0 };
            for &(x, v, c) in offsets.get(key).into_iter().flatten() {
                if x == axis && v * sign <= SPATIAL_MIN_SEGMENT {
                    let mut set: Vec<usize> = list.iter().map(|a| a.2).collect();
                    set.push(c);
                    conflicts.push(dependency(set));
                }
            }
        }
        for list in normals.values() {
            let (axis, sense, _) = list[0];
            if list.iter().any(|&(x, s, _)| x != axis || s != sense) {
                conflicts.push(dependency(list.iter().map(|a| a.2).collect()));
            }
        }
        for set in self.plane.directions().0 {
            conflicts.push(dependency(set));
        }
        (merge(conflicts), issues)
    }

    /// Geometry that the relations hold with but that is invalid: lines
    /// and curves without extent, alignments and tangencies pointing
    /// against their senses, an arc's midpoint on its complement.
    fn validate(&self) -> Vec<SpatialIssue> {
        let classes = self.classes();
        let mut issues = Vec::new();
        let mut collapsed = vec![false; self.lines.len()];
        for (i, _) in self.live_lines() {
            let l = SpatialLineId(i as u32);
            if self
                .line_length(l)
                .is_none_or(|len| len <= SPATIAL_MIN_SEGMENT)
            {
                collapsed[i] = true;
                issues.push(SpatialIssue::CollapsedLine(l));
            }
        }
        let mut degenerate = vec![false; self.curves.len()];
        for (i, data) in self.live_curves() {
            let id = SpatialCurveId(i as u32);
            let short = |a: [f64; 3], b: [f64; 3]| dist(a, b) <= SPATIAL_MIN_SEGMENT;
            let bad = match data {
                CurveData::Arc { s, e, .. } => {
                    self.radius(id).is_none_or(|r| r <= SPATIAL_MIN_SEGMENT)
                        || short(self.xyz(*s), self.xyz(*e))
                }
                CurveData::Circle { .. } => {
                    self.radius(id).is_none_or(|r| r <= SPATIAL_MIN_SEGMENT)
                }
                CurveData::Fitted { fit, ends, .. } => {
                    let pts: Vec<[f64; 3]> = fit.iter().map(|&p| self.xyz(p)).collect();
                    pts.windows(2).any(|w| short(w[0], w[1]))
                        || (ends[0] == SplineEnd::Periodic && short(pts[0], pts[pts.len() - 1]))
                }
                CurveData::BSpline { .. } => {
                    self.ctrl_columns(data)
                        .windows(2)
                        .map(|w| dist(self.col3(w[0]), self.col3(w[1])))
                        .sum::<f64>()
                        <= SPATIAL_MIN_SEGMENT
                }
                CurveData::Reference(_) => false,
                CurveData::Ellipse { ends, .. } => {
                    self.ellipse_radii(id)
                        .is_none_or(|(major, minor)| major.min(minor) <= SPATIAL_MIN_SEGMENT)
                        || ends.is_some_and(|[s, e]| short(self.xyz(s), self.xyz(e)))
                }
            };
            if bad {
                degenerate[i] = true;
                issues.push(SpatialIssue::DegenerateCurve(id));
            }
        }
        for (i, rec) in self.live_constraints() {
            let cid = SpatialConstraintId(i as u32);
            match rec.def {
                SpatialConstraint::Align { line, axis, sense } => {
                    if collapsed[line.index()] {
                        continue;
                    }
                    let [a, b] = self.lines[line.index()].expect("live line");
                    if self.delta(a, b)[axis.index()] * sense.sign() <= 0.0 {
                        issues.push(SpatialIssue::ReversedSense {
                            constraint: cid,
                            line,
                        });
                    }
                }
                SpatialConstraint::Along(SpatialAlong::Line { line, dir, sense }) => {
                    if collapsed[line.index()] {
                        continue;
                    }
                    let [a, b] = self.lines[line.index()].expect("live line");
                    let d = self.delta(a, b);
                    if (d[0] * dir[0] + d[1] * dir[1] + d[2] * dir[2]) * sense.sign() <= 0.0 {
                        issues.push(SpatialIssue::ReversedSense {
                            constraint: cid,
                            line,
                        });
                    }
                }
                SpatialConstraint::NormalAlign { curve, axis, sense } => {
                    let n = self.normal(curve).unwrap_or([0.0; 3]);
                    if n[axis.index()] * sense.sign() <= 0.0 {
                        issues.push(SpatialIssue::ReversedNormal {
                            constraint: cid,
                            curve,
                        });
                    }
                }
                SpatialConstraint::Tangent { a, b, at }
                | SpatialConstraint::Curvature { a, b, at } => {
                    let skip = |c: SpatialCurve| match c {
                        SpatialCurve::Line(l) => collapsed[l.index()],
                        SpatialCurve::Curve(id) => degenerate[id.index()],
                    };
                    if skip(a) || skip(b) {
                        continue;
                    }
                    let (u, v) = (
                        self.tan_value(a, at.index(), &classes).unwrap_or([0.0; 3]),
                        self.tan_value(b, at.index(), &classes).unwrap_or([0.0; 3]),
                    );
                    let mut zero = false;
                    for (c, t) in [(a, u), (b, v)] {
                        if curve::norm(t) < 0.5
                            && let SpatialCurve::Curve(id) = c
                        {
                            issues.push(SpatialIssue::DegenerateCurve(id));
                            zero = true;
                        }
                    }
                    if !zero && (u[0] * v[0] + u[1] * v[1] + u[2] * v[2]) * rec.sense <= 0.0 {
                        issues.push(SpatialIssue::ReversedTangent(cid));
                    }
                }
                SpatialConstraint::Midpoint {
                    point,
                    curve: SpatialCurve::Curve(curve),
                } => {
                    if degenerate[curve.index()] {
                        continue;
                    }
                    if let CurveData::Arc { c, s, e, normal } = *self.data(curve) {
                        let cv: V3<f64> = V3::cst(self.xyz(c));
                        let n: V3<f64> = V3::cst(self.normal_of(normal)).unit();
                        let rs = V3::cst(self.xyz(s)).sub(cv);
                        let full = sweep(n, rs, V3::cst(self.xyz(e)).sub(cv));
                        let half = sweep(n, rs, V3::cst(self.xyz(point.index())).sub(cv));
                        if half >= full {
                            issues.push(SpatialIssue::MidpointOffArc(cid));
                        }
                    }
                }
                _ => {}
            }
        }
        issues.sort_unstable();
        issues.dedup();
        issues
    }

    // Rows.

    fn nrm(&self, v: &mut Vars, n: NormalRec) -> Nrm {
        match n {
            NormalRec::Free(col) => Nrm::Var(v.p(col)),
            NormalRec::Axis(axis, sense) => Nrm::Const(axis.unit(sense)),
        }
    }

    /// The radius of an arc or circle as a row term.
    fn rad(&self, v: &mut Vars, data: &CurveData) -> Len {
        match *data {
            CurveData::Arc { c, s, .. } => Len::Dist(v.p(self.pc(c)), v.p(self.pc(s))),
            CurveData::Circle { r, .. } => Len::Param(v.v(r)),
            _ => unreachable!("validated as round"),
        }
    }

    /// The row that keeps point column `p` in the plane of a round with
    /// centre column `c`: `n.(p - c)`, or along a frame axis `p_k - c_k`.
    fn plane_row(&self, owner: Owner, normal: NormalRec, c: usize, p: usize) -> Row {
        match normal {
            NormalRec::Free(n) => row(owner, |v| RowKind::PlaneDot {
                n: Nrm::Var(v.p(n)),
                a: v.p(c),
                b: v.p(p),
            }),
            NormalRec::Axis(axis, _) => {
                let k = axis.index();
                row(owner, |v| RowKind::Diff {
                    a: v.v(c + k),
                    b: v.v(p + k),
                })
            }
        }
    }

    /// The spline columns of the support at `t` as a row term.
    fn spline_ref(
        &self,
        v: &mut Vars,
        basis: &Arc<SplineBasis>,
        ctrl: &[usize],
        t: f64,
    ) -> SplineRef {
        let range = basis.support(t);
        SplineRef {
            basis: basis.clone(),
            first: *range.start(),
            ctrl: range.map(|j| v.p(ctrl[j])).collect(),
        }
    }

    /// The unit tangent of `c` at point `at` (by index) as a row term:
    /// `None` for a reference curve or a spline that does not end at `at`.
    fn tan(&self, v: &mut Vars, c: SpatialCurve, at: usize, classes: &[usize]) -> Option<Tan> {
        match c {
            SpatialCurve::Line(l) => {
                let [a, b] = self.lines[l.index()]?;
                Some(Tan::Line(v.p(self.pc(a)), v.p(self.pc(b))))
            }
            SpatialCurve::Curve(id) => match self.curves[id.index()].as_ref()? {
                CurveData::Arc { c, normal, .. } | CurveData::Circle { c, normal, .. } => {
                    Some(Tan::Round {
                        n: self.nrm(v, *normal),
                        c: v.p(self.pc(*c)),
                        p: v.p(self.pc(at)),
                    })
                }
                data if data.is_spline() => {
                    let end = self.spline_end(data, at, classes)?;
                    let basis = self.spline_basis(data);
                    let (lo, hi) = basis.domain();
                    let t = if end == 0 { lo } else { hi };
                    let s = self.spline_ref(v, basis, &self.ctrl_columns(data), t);
                    Some(Tan::Spline { s, t })
                }
                _ => None,
            },
        }
    }

    /// A direction as a row term (mitcad#1661): a line's `unit(b - a)` or a
    /// round's normal.
    fn dir_tan(&self, v: &mut Vars, d: SpatialDir) -> Tan {
        match d {
            SpatialDir::Line(l) => {
                let [a, b] = self.lines[l.index()].expect("live line");
                Tan::Line(v.p(self.pc(a)), v.p(self.pc(b)))
            }
            SpatialDir::Normal(c) => Tan::Normal(self.nrm(v, self.round_normal(c))),
        }
    }

    /// The curvature vector of `c` at point `at` (by index) as a row term
    /// (mitcad#1661): none on a line, toward the centre on an arc or
    /// circle, at the end of a spline.
    fn crv(&self, v: &mut Vars, c: SpatialCurve, at: usize, classes: &[usize]) -> Crv {
        let SpatialCurve::Curve(id) = c else {
            return Crv::Line;
        };
        match self.data(id) {
            CurveData::Arc { c, .. } | CurveData::Circle { c, .. } => Crv::Round {
                c: v.p(self.pc(*c)),
                p: v.p(self.pc(at)),
            },
            data if data.is_spline() => {
                let end = self.spline_end(data, at, classes).unwrap_or(0);
                let basis = self.spline_basis(data);
                let (lo, hi) = basis.domain();
                let t = if end == 0 { lo } else { hi };
                Crv::Spline {
                    s: self.spline_ref(v, basis, &self.ctrl_columns(data), t),
                    t,
                }
            }
            _ => Crv::Line,
        }
    }

    /// The unit tangent of `c` at `at` now.
    fn tan_value(&self, c: SpatialCurve, at: usize, classes: &[usize]) -> Option<[f64; 3]> {
        let mut v = Vars::default();
        let tan = self.tan(&mut v, c, at, classes)?;
        let x: Vec<f64> = v.list.iter().map(|&g| self.params[g]).collect();
        Some(tan.eval(&x).val())
    }

    /// The internal rows of curve `i`.
    fn curve_rows(
        &self,
        rows: &mut Vec<Row>,
        i: usize,
        data: &CurveData,
        controlled: &HashSet<(usize, usize)>,
        in_plane: &HashSet<(usize, usize)>,
    ) {
        let owner = Owner::Curve(i);
        match data {
            CurveData::Arc { c, s, e, normal } => {
                let (cc, sc, ec) = (self.pc(*c), self.pc(*s), self.pc(*e));
                rows.push(row(owner, |v| RowKind::LenDiff {
                    a: Len::Dist(v.p(cc), v.p(sc)),
                    b: Len::Dist(v.p(cc), v.p(ec)),
                }));
                rows.push(self.plane_row(owner, *normal, cc, sc));
                rows.push(self.plane_row(owner, *normal, cc, ec));
                if let NormalRec::Free(n) = *normal {
                    let mut r = row(owner, |v| RowKind::Unit { n: v.p(n) });
                    r.size = vec![Size::Curve(i)];
                    rows.push(r);
                }
            }
            CurveData::Circle { normal, .. } => {
                if let NormalRec::Free(n) = *normal {
                    let mut r = row(owner, |v| RowKind::Unit { n: v.p(n) });
                    r.size = vec![Size::Curve(i)];
                    rows.push(r);
                }
            }
            CurveData::Fitted {
                basis,
                fit,
                params,
                ends,
                ..
            } => {
                let ctrl = self.ctrl_columns(data);
                // The interior fit points at their parameters (a closed
                // spline's last parameter is its first point's again).
                let k = params.len() - 1;
                for j in 1..k {
                    let (t, p) = (params[j], self.pc(fit[j]));
                    for axis in 0..3 {
                        rows.push(row(owner, |v| RowKind::OnSpline {
                            p: v.p(p),
                            s: self.spline_ref(v, basis, &ctrl, t),
                            t: Tp::Const(t),
                            k: axis,
                        }));
                    }
                }
                let (lo, hi) = basis.domain();
                for (end, t) in [(0, lo), (1, hi)] {
                    if ends[end] != SplineEnd::Natural || controlled.contains(&(i, end)) {
                        continue;
                    }
                    // An in-plane tangency controls x and y only.
                    let from = if in_plane.contains(&(i, end)) { 2 } else { 0 };
                    for axis in from..3 {
                        rows.push(row(owner, |v| RowKind::Deriv {
                            s: self.spline_ref(v, basis, &ctrl, t),
                            t,
                            order: 2,
                            k: axis,
                        }));
                    }
                }
                if ends[0] == SplineEnd::Periodic {
                    for order in 1..=2 {
                        for axis in 0..3 {
                            rows.push(row(owner, |v| RowKind::Periodic {
                                s: SplineRef {
                                    basis: basis.clone(),
                                    first: 0,
                                    ctrl: ctrl.iter().map(|&c| v.p(c)).collect(),
                                },
                                order,
                                k: axis,
                            }));
                        }
                    }
                }
            }
            CurveData::Ellipse { c, a, ends, .. } => {
                // The major point in the plane through the centre, and an
                // arc's ends on the ellipse.
                let cc = self.pc(*c);
                rows.push(self.ellipse_plane_row(owner, data, cc, self.pc(*a)));
                for &p in ends.iter().flatten() {
                    rows.push(self.ellipse_plane_row(owner, data, cc, self.pc(p)));
                    rows.push(self.ellipse_row(owner, i, data, self.pc(p)));
                }
            }
            CurveData::BSpline { .. } | CurveData::Reference(_) => {}
        }
    }

    /// The row that keeps point column `p` on ellipse `i` (in its plane:
    /// the plane row is separate), scaled by the ellipse's size.
    fn ellipse_row(&self, owner: Owner, i: usize, data: &CurveData, p: usize) -> Row {
        let CurveData::Ellipse {
            c, a, b, axis, dir, ..
        } = *data
        else {
            unreachable!("an ellipse");
        };
        let (cc, ac) = (self.pc(c), self.pc(a));
        let mut r = row(owner, |v| RowKind::OnEllipse {
            p: v.p(p),
            c: v.p(cc),
            a: v.p(ac),
            b: v.v(b),
            n: dir.unwrap_or_else(|| axis.unit(AxisSense::Positive)),
        });
        r.size = vec![Size::Curve(i)];
        r
    }

    /// The row that keeps point column `p` in the plane of an ellipse with
    /// centre column `c`: along its frame axis, or across a rotated plane's
    /// normal (mitcad#1661).
    fn ellipse_plane_row(&self, owner: Owner, data: &CurveData, c: usize, p: usize) -> Row {
        let CurveData::Ellipse { axis, dir, .. } = *data else {
            unreachable!("an ellipse");
        };
        match dir {
            Some(n) => row(owner, |v| RowKind::DirDiff {
                n,
                a: v.p(c),
                b: v.p(p),
            }),
            None => self.plane_row(owner, NormalRec::Axis(axis, AxisSense::Positive), c, p),
        }
    }

    /// The rows of relation `i`.
    #[allow(clippy::too_many_arguments)]
    fn constraint_rows(
        &self,
        rows: &mut Vec<Row>,
        i: usize,
        rec: &ConstraintRec,
        classes: &[usize],
        aligned: &BTreeMap<usize, (usize, f64)>,
        planar: &mut BTreeMap<usize, Vec<Equation>>,
    ) {
        use SpatialConstraint::*;
        let owner = Owner::Constraint(i);
        let src = |k: usize, factor: f64| {
            Some(ValueRef {
                constraint: i,
                k,
                factor,
            })
        };
        let diff = |a: usize, b: usize, value: f64, src: Option<ValueRef>| {
            let mut r = row(owner, |v| RowKind::Diff {
                a: v.v(a),
                b: v.v(b),
            });
            r.value = value;
            r.src = src;
            r
        };
        let target = |a: usize, value: f64, src: Option<ValueRef>| {
            let mut r = row(owner, |v| RowKind::Target { a: v.v(a) });
            r.value = value;
            r.src = src;
            r
        };
        let measured = |kind: RowKind, value: f64| {
            let mut r = row(owner, |_| kind);
            r.value = value;
            r.src = src(0, 1.0);
            r
        };
        let ends = |l: SpatialLineId| {
            let [a, b] = self.lines[l.index()].expect("live line");
            (self.pc(a), self.pc(b))
        };
        match rec.def {
            Coincident(a, b) => {
                let (ca, cb) = (self.pc(a.index()), self.pc(b.index()));
                for k in 0..3 {
                    rows.push(diff(ca + k, cb + k, 0.0, None));
                }
            }
            Fix { point, at: t } | Attach { point, target: t } => {
                let c = self.pc(point.index());
                for (k, &v) in t.iter().enumerate() {
                    rows.push(target(c + k, v, src(k, 1.0)));
                }
            }
            Align { line, axis, .. } => {
                let (a, b) = ends(line);
                for k in axis.transverse() {
                    rows.push(diff(a + k, b + k, 0.0, None));
                }
            }
            Offset {
                a,
                b,
                axis,
                value: v,
            } => {
                let j = axis.index();
                rows.push(diff(
                    self.pc(a.index()) + j,
                    self.pc(b.index()) + j,
                    v,
                    src(0, 1.0),
                ));
            }
            Length { line, value: v } => {
                // A line's first alignment turns its lengths into signed
                // axial differences (linear, and on the alignment's sense).
                let (a, b) = ends(line);
                match aligned.get(&line.index()) {
                    Some(&(k, s)) => rows.push(diff(a + k, b + k, s * v, src(0, s))),
                    None => {
                        let mut r = row(owner, |x| RowKind::Measure {
                            a: Len::Dist(x.p(a), x.p(b)),
                            k: 1.0,
                        });
                        r.value = v;
                        r.src = src(0, 1.0);
                        rows.push(r);
                    }
                }
            }
            PointOn { point, curve } => {
                let p = self.pc(point.index());
                match curve {
                    SpatialCurve::Line(l) => {
                        let ((a, b), t) = (ends(l), rec.params[0]);
                        for k in 0..3 {
                            rows.push(row(owner, |v| RowKind::OnLine {
                                p: v.p(p),
                                a: v.p(a),
                                b: v.p(b),
                                t: v.v(t),
                                k,
                            }));
                        }
                    }
                    SpatialCurve::Curve(id) => match self.data(id) {
                        data @ (CurveData::Arc { c, normal, .. }
                        | CurveData::Circle { c, normal, .. }) => {
                            let cc = self.pc(*c);
                            rows.push(self.plane_row(owner, *normal, cc, p));
                            rows.push(row(owner, |v| RowKind::LenDiff {
                                a: Len::Dist(v.p(cc), v.p(p)),
                                b: self.rad(v, data),
                            }));
                        }
                        CurveData::Reference(rc) => {
                            let t = rec.params[0];
                            for k in 0..3 {
                                rows.push(row(owner, |v| RowKind::OnReference {
                                    p: v.p(p),
                                    curve: rc.clone(),
                                    t: v.v(t),
                                    k,
                                }));
                            }
                        }
                        data @ CurveData::Ellipse { c, .. } => {
                            rows.push(self.ellipse_plane_row(owner, data, self.pc(*c), p));
                            rows.push(self.ellipse_row(owner, id.index(), data, p));
                        }
                        data => {
                            let (ctrl, basis, t) = (
                                self.ctrl_columns(data),
                                self.spline_basis(data),
                                rec.params[0],
                            );
                            for k in 0..3 {
                                rows.push(row(owner, |v| RowKind::OnSpline {
                                    p: v.p(p),
                                    s: SplineRef {
                                        basis: basis.clone(),
                                        first: 0,
                                        ctrl: ctrl.iter().map(|&c| v.p(c)).collect(),
                                    },
                                    t: Tp::Var(v.v(t)),
                                    k,
                                }));
                            }
                        }
                    },
                }
            }
            Midpoint { point, curve } => {
                let m = self.pc(point.index());
                match curve {
                    SpatialCurve::Line(l) => {
                        let (a, b) = ends(l);
                        for k in 0..3 {
                            rows.push(row(owner, |v| RowKind::Mid {
                                m: v.v(m + k),
                                a: v.v(a + k),
                                b: v.v(b + k),
                            }));
                        }
                    }
                    SpatialCurve::Curve(id) => match *self.data(id) {
                        CurveData::Arc { c, s, e, normal } => {
                            let (cc, sc, ec) = (self.pc(c), self.pc(s), self.pc(e));
                            rows.push(row(owner, |v| RowKind::LenDiff {
                                a: Len::Dist(v.p(sc), v.p(m)),
                                b: Len::Dist(v.p(ec), v.p(m)),
                            }));
                            rows.push(self.plane_row(owner, normal, cc, m));
                            rows.push(row(owner, |v| RowKind::LenDiff {
                                a: Len::Dist(v.p(cc), v.p(m)),
                                b: Len::Dist(v.p(cc), v.p(sc)),
                            }));
                        }
                        CurveData::Reference(ref rc) => {
                            let at = rc.midpoint().unwrap_or([0.0; 3]);
                            for (k, &v) in at.iter().enumerate() {
                                rows.push(target(m + k, v, None));
                            }
                        }
                        _ => {}
                    },
                }
            }
            Tangent { a, b, at } | Curvature { a, b, at } => {
                let at = at.index();
                let p = self.pc(at);
                let round = |c: SpatialCurve| match c {
                    SpatialCurve::Curve(id) if self.data(id).is_round() => Some(self.data(id)),
                    _ => None,
                };
                let is_line = |c: SpatialCurve| matches!(c, SpatialCurve::Line(_));
                // (the curve whose tangent is u, the round with c and n)
                let pair = match (round(a), round(b)) {
                    (None, Some(rb)) if is_line(a) => Some((a, b, rb)),
                    (Some(ra), None) if is_line(b) => Some((b, a, ra)),
                    (Some(_), Some(rb)) => Some((a, b, rb)),
                    _ => None,
                };
                match pair {
                    Some((u_of, r_of, data)) => {
                        let (centre, normal) = match *data {
                            CurveData::Arc { c, normal, .. }
                            | CurveData::Circle { c, normal, .. } => (self.pc(c), normal),
                            _ => unreachable!("a round"),
                        };
                        rows.push(row(owner, |v| RowKind::TanPoint {
                            u: self.tan(v, u_of, at, classes).expect("checked tangent"),
                            p: v.p(p),
                            c: v.p(centre),
                        }));
                        let mut r = row(owner, |v| RowKind::TanNormal {
                            u: self.tan(v, u_of, at, classes).expect("checked tangent"),
                            n: self.nrm(v, normal),
                        });
                        r.size = vec![Self::size(r_of)];
                        rows.push(r);
                    }
                    None => {
                        // The general form: u across both directions
                        // perpendicular to v, with the frame axis least
                        // aligned with v (chosen anew when the rows are
                        // prepared again).
                        let vv = self.tan_value(b, at, classes).unwrap_or([1.0, 0.0, 0.0]);
                        let axis = (0..3)
                            .min_by(|&x, &y| vv[x].abs().total_cmp(&vv[y].abs()))
                            .unwrap_or(0);
                        for second in [false, true] {
                            let mut r = row(owner, |v| RowKind::TanAxis {
                                u: self.tan(v, a, at, classes).expect("checked tangent"),
                                v: self.tan(v, b, at, classes).expect("checked tangent"),
                                axis,
                                second,
                            });
                            r.size = vec![Self::size(a), Self::size(b)];
                            rows.push(r);
                        }
                    }
                }
                if let Curvature { .. } = rec.def {
                    // Equal curvature vectors across the tangent of b.
                    let vv = self.tan_value(b, at, classes).unwrap_or([1.0, 0.0, 0.0]);
                    let axis = (0..3)
                        .min_by(|&x, &y| vv[x].abs().total_cmp(&vv[y].abs()))
                        .unwrap_or(0);
                    for second in [false, true] {
                        let mut r = row(owner, |v| RowKind::CurvAxis {
                            ka: self.crv(v, a, at, classes),
                            kb: self.crv(v, b, at, classes),
                            v: self.tan(v, b, at, classes).expect("checked tangent"),
                            axis,
                            second,
                        });
                        r.size = vec![Self::size(a), Self::size(b)];
                        rows.push(r);
                    }
                }
            }
            Parallel(a, b) => {
                let vv = self.direction(b).unwrap_or([1.0, 0.0, 0.0]);
                let axis = (0..3)
                    .min_by(|&x, &y| vv[x].abs().total_cmp(&vv[y].abs()))
                    .unwrap_or(0);
                for second in [false, true] {
                    let mut r = row(owner, |v| RowKind::TanAxis {
                        u: self.dir_tan(v, a),
                        v: self.dir_tan(v, b),
                        axis,
                        second,
                    });
                    r.size = vec![Self::size(a.curve()), Self::size(b.curve())];
                    rows.push(r);
                }
            }
            Perpendicular(a, b) => {
                let mut r = row(owner, |v| RowKind::DirDot {
                    u: self.dir_tan(v, a),
                    v: self.dir_tan(v, b),
                });
                r.size = vec![Self::size(a.curve()), Self::size(b.curve())];
                rows.push(r);
            }
            Angle { a, b, value } => {
                let mut r = row(owner, |v| RowKind::DirAngle {
                    u: self.dir_tan(v, a),
                    v: self.dir_tan(v, b),
                });
                r.value = value;
                r.src = src(0, 1.0);
                r.size = vec![Self::size(a.curve()), Self::size(b.curve())];
                rows.push(r);
            }
            Symmetric { a, b, plane } => {
                if let SpatialSurface::Plane { origin, normal } = plane {
                    let (ca, cb) = (self.pc(a.index()), self.pc(b.index()));
                    let mut r = row(owner, |v| RowKind::MidPlane {
                        a: v.p(ca),
                        b: v.p(cb),
                        n: normal,
                    });
                    r.value = (0..3).map(|k| normal[k] * origin[k]).sum();
                    rows.push(r);
                    for w in plane_across(normal) {
                        rows.push(row(owner, |v| RowKind::Across {
                            a: v.p(ca),
                            b: v.p(cb),
                            w,
                        }));
                    }
                }
            }
            Concentric { a, b, normal } => {
                let centre = |c: SpatialCurveId| match *self.data(c) {
                    CurveData::Arc { c, .. } | CurveData::Circle { c, .. } => c,
                    _ => unreachable!("validated as round"),
                };
                let (ca, cb) = (centre(a), centre(b));
                if classes[ca] != classes[cb] {
                    let (pa, pb) = (self.pc(ca), self.pc(cb));
                    for k in 0..3 {
                        rows.push(diff(pa + k, pb + k, 0.0, None));
                    }
                }
                let (na, nb) = (self.round_normal(a), self.round_normal(b));
                let both_fixed = matches!((na, nb), (NormalRec::Axis(..), NormalRec::Axis(..)));
                if normal && !both_fixed {
                    let (da, db) = (SpatialDir::Normal(a), SpatialDir::Normal(b));
                    let vv = self.direction(db).unwrap_or([0.0, 0.0, 1.0]);
                    let axis = (0..3)
                        .min_by(|&x, &y| vv[x].abs().total_cmp(&vv[y].abs()))
                        .unwrap_or(0);
                    for second in [false, true] {
                        let mut r = row(owner, |v| RowKind::TanAxis {
                            u: self.dir_tan(v, da),
                            v: self.dir_tan(v, db),
                            axis,
                            second,
                        });
                        r.size = vec![Size::Curve(a.index()), Size::Curve(b.index())];
                        rows.push(r);
                    }
                }
            }
            SplineLength { curve, value } => {
                let data = self.data(curve);
                let mut x = Vars::default();
                let s = SplineRef {
                    basis: self.spline_basis(data).clone(),
                    first: 0,
                    ctrl: self.ctrl_columns(data).iter().map(|&c| x.p(c)).collect(),
                };
                let mut r = measured(RowKind::SplineLength { s }, value);
                r.vars = x.list;
                rows.push(r);
            }
            TangentLength { curve, end, value } => {
                let data = self.data(curve);
                let basis = self.spline_basis(data);
                let (lo, hi) = basis.domain();
                let t = if end == 0 { lo } else { hi };
                let mut x = Vars::default();
                let s = self.spline_ref(&mut x, basis, &self.ctrl_columns(data), t);
                let mut r = measured(RowKind::TangentLength { s, t }, value);
                r.vars = x.list;
                rows.push(r);
            }
            Equal(a, b) => match (a, b) {
                (SpatialCurve::Line(la), SpatialCurve::Line(lb)) => {
                    let ((a1, b1), (a2, b2)) = (ends(la), ends(lb));
                    rows.push(row(owner, |v| RowKind::LenDiff {
                        a: Len::Dist(v.p(a1), v.p(b1)),
                        b: Len::Dist(v.p(a2), v.p(b2)),
                    }));
                }
                (SpatialCurve::Curve(ca), SpatialCurve::Curve(cb)) => {
                    let (da, db) = (self.data(ca), self.data(cb));
                    rows.push(row(owner, |v| RowKind::LenDiff {
                        a: self.rad(v, da),
                        b: self.rad(v, db),
                    }));
                }
                _ => {}
            },
            Radius { curve, value } | Diameter { curve, value } => {
                let k = if matches!(rec.def, Diameter { .. }) {
                    2.0
                } else {
                    1.0
                };
                let mut x = Vars::default();
                let a = self.rad(&mut x, self.data(curve));
                let mut r = measured(RowKind::Measure { a, k }, value);
                r.vars = x.list;
                rows.push(r);
            }
            ArcLength { curve, value } => {
                if let CurveData::Arc { c, s, e, normal } = *self.data(curve) {
                    let mut x = Vars::default();
                    let kind = RowKind::ArcLength {
                        c: x.p(self.pc(c)),
                        s: x.p(self.pc(s)),
                        e: x.p(self.pc(e)),
                        n: self.nrm(&mut x, normal),
                    };
                    let mut r = measured(kind, value);
                    r.vars = x.list;
                    rows.push(r);
                }
            }
            Along(along) => {
                // mitcad#1661: linear rows along, or across, a constant
                // direction.
                let across = plane_across(along.dir());
                match along {
                    SpatialAlong::Offset {
                        a,
                        b,
                        dir,
                        value: v,
                    } => {
                        let (pa, pb) = (self.pc(a.index()), self.pc(b.index()));
                        let mut r = row(owner, |x| RowKind::DirDiff {
                            n: dir,
                            a: x.p(pa),
                            b: x.p(pb),
                        });
                        r.value = v;
                        r.src = src(0, 1.0);
                        rows.push(r);
                    }
                    SpatialAlong::Line { line, .. } => {
                        let (a, b) = ends(line);
                        for w in across {
                            rows.push(row(owner, |x| RowKind::DirDiff {
                                n: w,
                                a: x.p(a),
                                b: x.p(b),
                            }));
                        }
                    }
                    SpatialAlong::Normal { curve, .. } => {
                        if let CurveData::Arc {
                            normal: NormalRec::Free(n),
                            ..
                        }
                        | CurveData::Circle {
                            normal: NormalRec::Free(n),
                            ..
                        } = *self.data(curve)
                        {
                            for w in across {
                                let mut r = row(owner, |x| RowKind::NormalDot { n: x.p(n), w });
                                r.size = vec![Size::Curve(curve.index())];
                                rows.push(r);
                            }
                        }
                    }
                }
            }
            NormalAlign { curve, axis, .. } => {
                if let CurveData::Arc {
                    normal: NormalRec::Free(n),
                    ..
                }
                | CurveData::Circle {
                    normal: NormalRec::Free(n),
                    ..
                } = *self.data(curve)
                {
                    for k in axis.transverse() {
                        let mut r = row(owner, |v| RowKind::NormalComp { n: v.p(n), k });
                        r.size = vec![Size::Curve(curve.index())];
                        rows.push(r);
                    }
                }
            }
            EllipseRadius {
                curve,
                minor,
                value,
            } => {
                if let CurveData::Ellipse { c, a, b, .. } = *self.data(curve) {
                    let mut x = Vars::default();
                    let a = if minor {
                        Len::Param(x.v(b))
                    } else {
                        Len::Dist(x.p(self.pc(c)), x.p(self.pc(a)))
                    };
                    let mut r = measured(RowKind::Measure { a, k: 1.0 }, value);
                    r.vars = x.list;
                    rows.push(r);
                }
            }
            OnSketchPlane(point) => rows.push(target(self.pc(point.index()) + 2, 0.0, None)),
            OnSurface { point, surface } => {
                let c = self.pc(point.index());
                rows.push(row(owner, |v| RowKind::OnSurface { p: v.p(c), surface }));
            }
            InPlane(_) => {
                for eq in planar.remove(&i).unwrap_or_default() {
                    // Every shadow unknown a relation uses has a column
                    // (or, on a rotated plane, a virtual one: mitcad#1661).
                    let virt = |g: usize| self.plane.virtual_of(g);
                    if eq
                        .vars
                        .iter()
                        .any(|&g| g >= self.params.len() && virt(g).is_none())
                    {
                        continue;
                    }
                    let value_src = match eq.src {
                        ValueSrc::Dim(_) => src(0, 1.0),
                        _ => None,
                    };
                    let fixed = eq.src == ValueSrc::Fixed;
                    if eq.vars.iter().any(|&g| virt(g).is_some()) {
                        let mut x = Vars::default();
                        let map: Vec<FrameVar> = eq
                            .vars
                            .iter()
                            .map(|&g| match virt(g) {
                                Some((col, e)) => FrameVar::Dot { p: x.p(col), e },
                                None => FrameVar::Col(x.v(g)),
                            })
                            .collect();
                        rows.push(Row {
                            owner,
                            vars: x.list,
                            kind: RowKind::FramePlanar { kind: eq.kind, map },
                            value: eq.value,
                            src: value_src,
                            fixed,
                            l: 1.0,
                            size: Vec::new(),
                        });
                        continue;
                    }
                    rows.push(Row {
                        owner,
                        vars: eq.vars,
                        kind: RowKind::Planar(eq.kind),
                        value: eq.value,
                        src: match eq.src {
                            ValueSrc::Dim(_) => src(0, 1.0),
                            _ => None,
                        },
                        fixed: eq.src == ValueSrc::Fixed,
                        l: 1.0,
                        size: Vec::new(),
                    });
                }
            }
        }
    }

    /// The rows of the relations in creation order and of the curves, their
    /// components and the free columns.
    fn prepare(&mut self) -> Prepared {
        self.sync_plane();
        let classes = self.classes();
        let mut aligned: BTreeMap<usize, (usize, f64)> = BTreeMap::new();
        // Fitted spline ends a spatial tangency controls (all three natural
        // rows go).
        let mut controlled: HashSet<(usize, usize)> = HashSet::new();
        for (_, rec) in self.live_constraints() {
            match rec.def {
                SpatialConstraint::Align { line, axis, sense } => {
                    aligned
                        .entry(line.index())
                        .or_insert((axis.index(), sense.sign()));
                }
                SpatialConstraint::Tangent { a, b, at }
                | SpatialConstraint::Curvature { a, b, at } => {
                    for c in [a, b] {
                        if let SpatialCurve::Curve(id) = c
                            && let Some(end) = self.spline_end(self.data(id), at.index(), &classes)
                        {
                            controlled.insert((id.index(), end));
                        }
                    }
                }
                _ => {}
            }
        }
        let (planar, in_plane) = if self.plane.is_empty() {
            (Vec::new(), HashSet::new())
        } else {
            self.plane.compile()
        };
        let mut by_owner: BTreeMap<usize, Vec<Equation>> = BTreeMap::new();
        for (owner, eq) in planar {
            by_owner.entry(owner).or_default().push(eq);
        }
        let mut rows = Vec::new();
        for (i, rec) in self.live_constraints() {
            self.constraint_rows(&mut rows, i, rec, &classes, &aligned, &mut by_owner);
        }
        for (i, data) in self.live_curves() {
            self.curve_rows(&mut rows, i, data, &controlled, &in_plane);
        }
        let n = self.params.len();
        let vars: Vec<&[usize]> = rows.iter().map(|r| r.vars.as_slice()).collect();
        let (comps, used) = connected_components(n, &vars, &self.weights);
        let free = self
            .live_columns()
            .into_iter()
            .filter(|&g| !used[g])
            .collect();
        Prepared { rows, comps, free }
    }

    fn ensure_prepared(&mut self) {
        if self.prepared.is_none() {
            let prep = self.prepare();
            self.prepared = Some(prep);
        }
    }

    /// True when a general tangency's auxiliary axis turned too close to
    /// its tangent (`abs(v.e) > 0.9`): the rows are prepared again.
    fn needs_rebuild(&self, prep: &Prepared) -> bool {
        prep.rows.iter().any(|r| match &r.kind {
            RowKind::TanAxis { v, axis, .. } | RowKind::CurvAxis { v, axis, .. } => {
                let x: Vec<f64> = r.vars.iter().map(|&g| self.params[g]).collect();
                v.eval(&x).val()[*axis].abs() > 0.9
            }
            _ => false,
        })
    }

    /// Constant per solve: the rows' characteristic lengths, the weights of
    /// curve parameters (their curve's length over the parameter range) and
    /// normals (their radius), and planar fixes' values.
    fn refresh(&mut self, prep: &mut Prepared) {
        for r in &mut prep.rows {
            if !r.size.is_empty() {
                r.l = r.size.iter().map(|&s| self.size_of(s)).fold(1.0, f64::max);
            }
            if r.fixed
                && let RowKind::Planar(EqKind::Fixed { a }) = r.kind
            {
                r.value = self.params[r.vars[a]];
            }
            // A planar Fix on a rotated plane (mitcad#1661): the point's
            // coordinate along the plane's axis where the solve starts.
            if r.fixed
                && let RowKind::FramePlanar {
                    kind: EqKind::Fixed { a },
                    map,
                } = &r.kind
            {
                let at = |i: usize| self.params[r.vars[i]];
                r.value = match &map[*a] {
                    FrameVar::Col(i) => at(*i),
                    FrameVar::Dot { p, e } => e[0] * at(p[0]) + e[1] * at(p[1]) + e[2] * at(p[2]),
                };
            }
        }
        let mut weights: Vec<(usize, f64)> = Vec::new();
        for (_, rec) in self.live_constraints() {
            if let (SpatialConstraint::PointOn { curve, .. }, Some(&t)) =
                (rec.def, rec.params.first())
            {
                let span = self.bounds[t].map_or(1.0, |(lo, hi)| hi - lo);
                let len = self.size_of(Self::size(curve));
                let span = if span > 0.0 { span } else { 1.0 };
                weights.push((t, (len / span).powi(2)));
            }
        }
        for (i, data) in self.live_curves() {
            if let CurveData::Arc {
                normal: NormalRec::Free(n),
                ..
            }
            | CurveData::Circle {
                normal: NormalRec::Free(n),
                ..
            } = *data
            {
                let r = self.size_of(Size::Curve(i));
                weights.extend((n..n + 3).map(|g| (g, r * r)));
            }
        }
        for (g, w) in weights {
            self.weights[g] = w;
        }
        for comp in &mut prep.comps {
            for (k, &g) in comp.vars.iter().enumerate() {
                comp.dcol[k] = 1.0 / self.weights[g];
            }
        }
    }

    /// Characteristic size: the diagonal of the box around the points,
    /// the targets and `extra` (drag targets) in local coordinates, at
    /// least the largest length, offset or radius and 1 mm. An extent, not
    /// a distance from the origin: far from it the size stays that of the
    /// sketch.
    fn scale(&self, extra: &[[f64; 3]]) -> f64 {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        let mut grow = |v: [f64; 3]| {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        };
        for p in self.points() {
            grow(self.xyz(p.index()));
        }
        let mut size: f64 = 0.0;
        for (_, rec) in self.live_constraints() {
            match rec.def {
                SpatialConstraint::Fix { at: t, .. }
                | SpatialConstraint::Attach { target: t, .. } => grow(t),
                SpatialConstraint::Length { value, .. }
                | SpatialConstraint::Offset { value, .. }
                | SpatialConstraint::Along(SpatialAlong::Offset { value, .. })
                | SpatialConstraint::Radius { value, .. }
                | SpatialConstraint::Diameter { value, .. }
                | SpatialConstraint::EllipseRadius { value, .. }
                | SpatialConstraint::SplineLength { value, .. }
                | SpatialConstraint::TangentLength { value, .. }
                | SpatialConstraint::ArcLength { value, .. } => size = size.max(value.abs()),
                _ => {}
            }
        }
        for (_, data) in self.live_curves() {
            match *data {
                CurveData::Circle { r, .. } => size = size.max(2.0 * self.params[r].abs()),
                CurveData::Ellipse { b, .. } => size = size.max(2.0 * self.params[b].abs()),
                _ => {}
            }
        }
        for &v in extra {
            grow(v);
        }
        if lo[0].is_finite() {
            let d = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
            size = size.max((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt());
        }
        size.max(1.0)
    }

    /// Largest residual of the current geometry (millimetres).
    pub fn residual(&mut self) -> f64 {
        self.ensure_prepared();
        let mut prep = self.prepared.take().expect("prepared");
        self.refresh(&mut prep);
        let res = self.residual_of(&prep);
        self.prepared = Some(prep);
        res
    }

    fn residual_of(&self, prep: &Prepared) -> f64 {
        let eqs = SpatialEquations {
            rows: &prep.rows,
            bounds: &self.bounds,
        };
        let scale = self.scale(&[]);
        let mut res: f64 = 0.0;
        for comp in &prep.comps {
            let mut f = vec![0.0; comp.eqs.len()];
            eqs.evaluate(comp, &self.params, scale, &mut f, None);
            res = f.iter().fold(res, |m, x| m.max(x.abs()));
        }
        res
    }

    /// Solves every relation, moving the geometry as little as possible.
    /// Anything but [`SpatialStatus::Converged`] leaves every unknown where
    /// it was.
    pub fn solve(&mut self, opts: &SpatialOptions) -> SpatialSolveResult {
        self.run(opts, &[])
    }

    /// Moves points toward local targets as far as the relations allow
    /// (relations hard, goals soft), moving other geometry as little as
    /// possible; a coordinate that no relation uses goes to its target. A
    /// drag that fails leaves every unknown where it was.
    pub fn drag(
        &mut self,
        goals: &[SpatialDragGoal],
        opts: &SpatialOptions,
    ) -> Result<SpatialSolveResult, SpatialError> {
        self.drag_with(goals, &[], opts)
    }

    /// [`SpatialSystem::drag`] with fit-point splines' end tangents as soft
    /// goals too ([`SpatialTangentGoal`]: a goal on the hidden control
    /// point next to the end, which sets `C'(t)` there).
    pub fn drag_with(
        &mut self,
        goals: &[SpatialDragGoal],
        tangents: &[SpatialTangentGoal],
        opts: &SpatialOptions,
    ) -> Result<SpatialSolveResult, SpatialError> {
        let mut columns: Vec<(usize, f64)> = Vec::new();
        for g in goals {
            let i = self.pidx(g.point)?;
            if !finite3(g.target) {
                return Err(SpatialError::InvalidValue("a drag target is not finite"));
            }
            let c = self.pc(i);
            columns.extend((0..3).map(|k| (c + k, g.target[k])));
        }
        for g in tangents {
            let data = self.cidx(g.spline)?;
            let CurveData::Fitted {
                basis,
                fit,
                hidden,
                ends,
                ..
            } = data
            else {
                return Err(SpatialError::InvalidValue(
                    "a tangent goal of a fit-point spline",
                ));
            };
            if g.end > 1 || ends[0] == SplineEnd::Periodic || !finite3(g.target) {
                return Err(SpatialError::InvalidValue(
                    "a tangent goal at an end (0 or 1) of an open spline, finite",
                ));
            }
            // C'(lo) = p (P1 - P0) / (u_{p+1} - u_1), C'(hi) = p (Pn -
            // Pn-1) / (u_{n+p} - u_n): the hidden point next to the end.
            let (p, k, n) = (basis.degree, &basis.knots, basis.n_ctrl() - 1);
            let (end_point, col, factor) = if g.end == 0 {
                (fit[0], hidden[0], (k[p + 1] - k[1]) / p as f64)
            } else {
                (
                    fit[fit.len() - 1],
                    hidden[hidden.len() - 1],
                    -(k[n + p] - k[n]) / p as f64,
                )
            };
            let at = self.xyz(end_point);
            columns.extend((0..3).map(|j| (col + j, at[j] + factor * g.target[j])));
        }
        Ok(self.run(opts, &columns))
    }

    fn outcome(status: SpatialStatus, residual: f64) -> SpatialSolveResult {
        SpatialSolveResult {
            status,
            iterations: 0,
            residual,
            conflicts: Vec::new(),
            issues: Vec::new(),
        }
    }

    /// Solves with soft goals on columns (`(column, target)`; none for a
    /// plain solve).
    fn run(&mut self, opts: &SpatialOptions, goal_params: &[(usize, f64)]) -> SpatialSolveResult {
        let (conflicts, issues) = self.check_structure();
        if !conflicts.is_empty() || !issues.is_empty() {
            let residual = self.residual();
            let status = if conflicts.is_empty() {
                SpatialStatus::Invalid
            } else {
                SpatialStatus::Conflicting
            };
            return SpatialSolveResult {
                conflicts,
                issues,
                ..Self::outcome(status, residual)
            };
        }
        self.ensure_prepared();
        if self
            .prepared
            .as_ref()
            .is_some_and(|p| self.needs_rebuild(p))
        {
            self.invalidate();
            self.ensure_prepared();
        }
        let mut prep = self.prepared.take().expect("prepared");
        self.refresh(&mut prep);
        let original = self.params.clone();
        let tol = opts.tolerance;
        let targets: Vec<[f64; 3]> = goal_params
            .chunks(3)
            .filter(|c| c.len() == 3)
            .map(|c| [c[0].1, c[1].1, c[2].1])
            .collect();
        let scale = self.scale(&targets);
        // Goals on columns without rows move freely.
        for &(g, t) in goal_params {
            if prep.free.binary_search(&g).is_ok() {
                self.params[g] = t;
            }
        }
        // Continuation: large value changes are applied in steps.
        let mut changed: Vec<(usize, usize, f64, f64)> = Vec::new();
        let mut steps = 1usize;
        for (i, rec) in self.live_constraints() {
            for (k, (v, last)) in self.values(&rec.def).into_iter().zip(&rec.last).enumerate() {
                if (v - last).abs() <= tol {
                    continue;
                }
                changed.push((i, k, *last, v));
                let n =
                    ((v - last).abs() / (0.3 * v.abs().max(last.abs()).max(1e-6 * scale))).ceil();
                steps = steps.max(n as usize);
            }
        }
        if !opts.continuation {
            steps = 1;
        }
        let steps = steps.min(16);
        let start = self.params.clone();
        let mut result = self.run_steps(&mut prep, opts, scale, goal_params, &changed, steps);
        if result.status != SpatialStatus::Converged && steps > 1 {
            // Retry in one step from the start.
            self.params.clone_from(&start);
            result = self.run_steps(&mut prep, opts, scale, goal_params, &changed, 1);
        }
        self.prepared = Some(prep);
        if result.status == SpatialStatus::Converged {
            result.issues = self.validate();
            if !result.issues.is_empty() {
                result.status = SpatialStatus::Invalid;
            }
        }
        if result.status == SpatialStatus::Converged {
            let values: Vec<(usize, Vec<f64>)> = self
                .live_constraints()
                .map(|(i, rec)| (i, self.values(&rec.def)))
                .collect();
            for (i, v) in values {
                if let Some(rec) = &mut self.constraints[i] {
                    rec.last = v;
                }
            }
        } else {
            self.params = original;
        }
        self.plane.sync(&self.params);
        result
    }

    fn run_steps(
        &mut self,
        prep: &mut Prepared,
        opts: &SpatialOptions,
        scale: f64,
        goal_params: &[(usize, f64)],
        changed: &[(usize, usize, f64, f64)],
        steps: usize,
    ) -> SpatialSolveResult {
        let mut result = Self::outcome(SpatialStatus::Converged, 0.0);
        for step in 1..=steps {
            // A general tangency's axis that turned too close to its
            // tangent in the last step: the rows are prepared again.
            if step > 1 && self.needs_rebuild(prep) {
                *prep = self.prepare();
                self.refresh(prep);
            }
            let t = step as f64 / steps as f64;
            for &(c, k, from, to) in changed {
                prep.update_value(c, k, from + (to - from) * t);
            }
            result = self.solve_components(prep, opts, scale, goal_params);
            if result.status != SpatialStatus::Converged {
                break;
            }
        }
        // Leave the rows at the target values.
        for &(c, k, _, to) in changed {
            prep.update_value(c, k, to);
        }
        result
    }

    fn solve_components(
        &mut self,
        prep: &Prepared,
        opts: &SpatialOptions,
        scale: f64,
        goal_params: &[(usize, f64)],
    ) -> SpatialSolveResult {
        let eqs = SpatialEquations {
            rows: &prep.rows,
            bounds: &self.bounds,
        };
        let ctx = Ctx {
            eqs: &eqs,
            scale,
            tol: opts.tolerance,
            max_iterations: opts.max_iterations,
        };
        let mut result = Self::outcome(SpatialStatus::Converged, 0.0);
        let mut singular = Vec::new();
        let mut params = std::mem::take(&mut self.params);
        for comp in &prep.comps {
            let goals: Vec<(usize, f64)> = goal_params
                .iter()
                .filter_map(|&(g, t)| comp.vars.binary_search(&g).ok().map(|c| (c, t)))
                .collect();
            let mut work = Work::new(comp);
            let run = if goals.is_empty() {
                solve::lm(&ctx, comp, &mut params, &mut work)
            } else {
                solve::drag(&ctx, comp, &mut params, &mut work, &goals)
            };
            result.iterations = result.iterations.max(run.iterations);
            result.residual = result.residual.max(run.residual);
            if run.converged {
                continue;
            }
            // Explain the failure at the least-squares point.
            let a = analysis::component_dependencies(&eqs, comp, &params, scale, &mut work, false);
            let (found, sing) = dependencies(
                &prep.rows,
                comp,
                &work.jac,
                &a.dependent,
                opts.tolerance,
                true,
            );
            if !found.is_empty() {
                result.status = SpatialStatus::Conflicting;
                result.conflicts.extend(found);
            } else if result.status == SpatialStatus::Converged {
                result.status = SpatialStatus::NotConverged;
            }
            singular.extend(sing);
        }
        self.params = params;
        result.conflicts = merge(result.conflicts);
        result.issues = singular_issues(singular);
        result
    }

    /// Degrees of freedom, fully determined points, lines and curves, and
    /// redundant or conflicting constraints at the current geometry (call
    /// it after a successful [`SpatialSystem::solve`]). A point is
    /// determined when its three coordinates are (the row-space test of the
    /// rank analysis), a line when its points are, an arc when its points
    /// and normal are, a circle with its radius, a spline when its points
    /// (and so its free end tangents) are; curve parameters are auxiliary.
    /// Nothing is reported fully constrained unless every row holds within
    /// [`SPATIAL_TOLERANCE`], the geometry is valid and nothing conflicts.
    pub fn analyze(&mut self) -> SpatialAnalysis {
        let (mut conflicts, mut issues) = self.check_structure();
        self.ensure_prepared();
        let mut prep = self.prepared.take().expect("prepared");
        if self.needs_rebuild(&prep) {
            prep = self.prepare();
        }
        self.refresh(&mut prep);
        let eqs = SpatialEquations {
            rows: &prep.rows,
            bounds: &self.bounds,
        };
        let scale = self.scale(&[]);
        let mut rank = 0;
        let mut dof = prep.free.len();
        let mut determined = vec![false; self.params.len()];
        let mut redundant = Vec::new();
        let mut singular = Vec::new();
        for comp in &prep.comps {
            let mut work = Work::new(comp);
            let a =
                analysis::component_dependencies(&eqs, comp, &self.params, scale, &mut work, true);
            rank += a.rank;
            dof += comp.vars.len() - a.rank;
            if let Some(det) = &a.determined {
                for (k, &g) in comp.vars.iter().enumerate() {
                    determined[g] = det[k];
                }
            }
            let (found, sing) = dependencies(
                &prep.rows,
                comp,
                &work.jac,
                &a.dependent,
                SPATIAL_TOLERANCE,
                true,
            );
            conflicts.extend(found);
            singular.extend(sing);
            let (found, _) = dependencies(
                &prep.rows,
                comp,
                &work.jac,
                &a.dependent,
                SPATIAL_TOLERANCE,
                false,
            );
            redundant.extend(found);
        }
        for set in self.plane.directions().1 {
            redundant.push(dependency(set));
        }
        issues.extend(singular_issues(singular));
        let solved = self.residual_of(&prep) <= SPATIAL_TOLERANCE;
        self.prepared = Some(prep);
        issues.extend(self.validate());
        issues.sort_unstable();
        issues.dedup();
        let conflicts = merge(conflicts);
        let redundant: Vec<SpatialDependency> = merge(redundant)
            .into_iter()
            .filter(|d| !conflicts.iter().any(|c| c.constraint == d.constraint))
            .collect();
        let valid = solved && conflicts.is_empty() && issues.is_empty();
        let col_done = |g: usize| valid && determined[g];
        let point_done = |p: usize| {
            let c = self.pc(p);
            (0..3).all(|k| col_done(c + k))
        };
        let fully_constrained_points = self.points().filter(|p| point_done(p.index())).collect();
        let fully_constrained_lines = self
            .live_lines()
            .filter(|&(_, [a, b])| point_done(a) && point_done(b))
            .map(|(i, _)| SpatialLineId(i as u32))
            .collect();
        let fully_constrained_curves = self
            .live_curves()
            .filter(|(_, data)| {
                valid
                    && data.points().into_iter().all(point_done)
                    && data.own_columns().into_iter().all(col_done)
            })
            .map(|(i, _)| SpatialCurveId(i as u32))
            .collect();
        SpatialAnalysis {
            unknowns: self.live_columns().len(),
            rank,
            dof,
            solved,
            fully_constrained_points,
            fully_constrained_lines,
            fully_constrained_curves,
            redundant,
            conflicts,
            issues,
        }
    }
}
