// SPDX-License-Identifier: MIT
//! 3D sketches (mitcad#1521): points with three coordinates and curves
//! through them (lines, arcs, circles, splines; plane-bound ellipses and
//! texts, kept from a converted planar sketch or drawn on a frame plane,
//! mitcad#1550), the stored definition of the
//! `spatial_sketch` feature, its adapter to the solver's spatial system
//! ([`mitcad_solver::SpatialSystem`]) and the evaluated output. The model
//! evaluates the feature (`mitcad_model::features::spatial_sketch`);
//! [`crate::spatial_edit`] edits the definition.
//!
//! A 3D sketch is separate from the planar sketch ([`crate::def::SketchDef`]),
//! whose entities, commands and regions keep their meaning:
//!
//! - The frame: the support (an origin plane, a construction plane or a
//!   planar face of the sketch's component, found as a planar sketch finds
//!   its plane) gives an origin `O` and unit axes `x`, `y` and `n = x × y`
//!   at right angles ([`orthonormal_frame`]). A point's local coordinates
//!   `p` are at `O + p0 x + p1 y + p2 n` in the component. The camera and
//!   temporary drawing planes never change it.
//! - Points (`p<n>`), curves (`c<n>`) and texts (`t<n>`) share one id
//!   space, constraints and dimensions (`k<n>`) another, as in a planar
//!   sketch. A curve refers to the points that define it
//!   ([`SpatialCurveKind`]): a line to its two ends, an arc to its centre,
//!   start and end (counter-clockwise about its unit normal, which is free
//!   or bound to a frame axis), a circle to its centre, a spline to its
//!   control or fit points (a fit-point spline's hidden control points are
//!   the solver's, made from the fit points at every solve). Curves that
//!   share a point id are joined there. Construction curves are kept apart
//!   from routes by their flag. A reference curve is constant geometry from
//!   outside (an edge, another sketch's curve, an Intersection Curve: one
//!   record per source and occurrence), supplied when the sketch is built
//!   ([`SpatialInputs`]), in true 3D or projected onto the sketch's plane.
//! - A point's stored position, an arc's or circle's stored normal and a
//!   circle's radius are where the last solve left them and where the next
//!   one starts: entering coordinates fixes nothing.
//! - Relations ([`SpatialConstraintKind`]): coincidence of two separately
//!   made points, Fix (a deliberate local anchor), Attach (to a source
//!   point, [`SpatialSource`]: the origin, a vertex, an edge's or another
//!   sketch's curve's start, end or middle, another sketch's point, of the
//!   same component or of another through its occurrence; its target found
//!   before the solve, mitcad#1543), alignment of a
//!   line or of an arc's normal with a frame axis in an explicit sense,
//!   point on a curve, midpoint, tangency (with the sense it was made
//!   with), equal lengths or radii, membership of the sketch plane, the
//!   in-plane relations of a sketch converted from a planar one (planar
//!   constraints solved on the points' x and y), and a point on a face of a
//!   body (On Face, mitcad#1549: the face's surface in local coordinates
//!   found before the solve, [`SpatialInputs::surfaces`]); and
//!   (mitcad#1661) parallel and perpendicular directions (lines' and
//!   arcs' or circles' normals, no intersection implied), symmetry of two
//!   points about a plane ([`SpatialPlaneRef`]: a frame plane, a
//!   construction plane, a planar face, another sketch's plane, the
//!   outside ones found before the solve), concentric arcs and circles
//!   (with parallel planes or the centre only), curvature continuity (G2)
//!   at a spline's end and points on such a plane (On Plane).
//! - Dimensions ([`SpatialDimensionKind`]): a line's true length, the
//!   signed offset between two points along an axis, an arc's or circle's
//!   radius or diameter, an arc's length, a spline's length, the angle
//!   between two directions, a spline end's tangent magnitude
//!   (mitcad#1661) and in-plane planar dimensions; driving (the value is a
//!   parameter) or driven (only measured, its value a read-only reference
//!   parameter, mitcad#1661). A driving dimension may use a driven one's
//!   reference: the model solves the sketch in stages
//!   (`mitcad_model::features::spatial_sketch`).
//! - Profile regions (mitcad#1550): closed loops in the sketch plane bound
//!   regions as a planar sketch's curves do, with the same keys
//!   ([`crate::spatial_planar::planar_view`]); loops out of the plane bound
//!   none. A converted planar sketch's planar records
//!   ([`crate::spatial_planar::PlanarRecords`]) act in the plane.
//!
//! Lengths are millimetres; the solve is accepted at an absolute
//! [`mitcad_solver::SPATIAL_TOLERANCE`], and a line, a radius or the
//! spacing of an arc's ends or a spline's points at most
//! [`SPATIAL_MIN_SEGMENT`] is invalid.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;
use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

// Relations along constant directions (mitcad#1661: a rotated placement
// plane).
use mitcad_solver::SpatialAlong;
use mitcad_solver::{
    Anchor as SolverAnchor, Axis, AxisSense, Constraint as PlanarRow, EntityId, PointId,
    SPATIAL_MIN_SEGMENT, SpatialConstraint, SpatialConstraintId, SpatialCurve as SolverCurve,
    SpatialCurveId, SpatialDir, SpatialDragGoal, SpatialEntityKind, SpatialIssue, SpatialNormal,
    SpatialOptions, SpatialPointId, SpatialSolveResult, SpatialStatus, SpatialSystem,
    SpatialTangentGoal, SplineEnd,
};
// The constant curves and solved spline geometry of the public API.
pub use mitcad_solver::{SpatialReferenceCurve, SpatialSplineGeometry};
// The surfaces points lie on (mitcad#1549).
pub use mitcad_solver::SpatialSurface;
use mitcad_vec3::{cross, dot, norm, sub};
// What a person reads and what agents read of a refusal (mitcad#1644).
use mitcad_model_base::notice::{FeatureError, NoticePlace, said_after, said_at};

use crate::datum::SurfaceGeometry;
use crate::def::{FrameDef, SketchDef, SketchPlane};
use crate::geometry::Curve2;
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::{Curve3, Kernel};
use crate::occurrence::OccurrenceLink;
use crate::parameters::ParamId;
use crate::pattern::Binding;
use crate::point_serde;
use crate::profile::SketchFrame;
use crate::solve::{
    PlanarIds, Solved, SpacingDef, binding_refs, measure_in, planar_binding, planar_constraint,
    planar_dimension,
};
use crate::spatial_planar::PlanarRecords;
use crate::topo::{EdgeName, FaceName, VertexName};
use crate::{
    Constraint, ConstraintKind, ConstraintUid, DimensionKind, Ref, SketchEntityRef, SketchText,
    SpacingMode, ValueRange,
};

/// An axis of a 3D sketch's frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialAxis {
    X,
    Y,
    Z,
}

impl SpatialAxis {
    /// 0, 1 or 2.
    pub fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }

    /// `X`, `Y` or `Z`.
    pub fn name(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
        }
    }

    /// The axis of index 0, 1 or 2.
    pub fn of_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::X),
            1 => Some(Self::Y),
            2 => Some(Self::Z),
            _ => None,
        }
    }

    fn solver(self) -> Axis {
        match self {
            Self::X => Axis::X,
            Self::Y => Axis::Y,
            Self::Z => Axis::Z,
        }
    }

    /// The two frame axes (indices) of the plane square to this axis, as
    /// that plane's x and y (mitcad#1550): XY's x and y for Z, YZ's y and
    /// z for X, ZX's z and x for Y, so that x × y points along this axis.
    pub fn plane_axes(self) -> [usize; 2] {
        match self {
            Self::X => [1, 2],
            Self::Y => [2, 0],
            Self::Z => [0, 1],
        }
    }

    /// `YZ`, `ZX` or `XY`: the frame plane square to this axis.
    pub fn plane_name(self) -> &'static str {
        match self {
            Self::X => "YZ",
            Self::Y => "ZX",
            Self::Z => "XY",
        }
    }

    /// The axis square to a frame plane named `xy`, `yz` or `zx` (any
    /// case).
    pub fn of_plane(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "xy" => Some(Self::Z),
            "yz" => Some(Self::X),
            "zx" => Some(Self::Y),
            _ => None,
        }
    }

    /// A point of the plane square to this axis at planar `(u, v)` and
    /// `elevation` along the axis, in local coordinates.
    pub fn place(self, uv: [f64; 2], elevation: f64) -> [f64; 3] {
        let [i, j] = self.plane_axes();
        let mut p = [0.0; 3];
        p[i] = uv[0];
        p[j] = uv[1];
        p[self.index()] = elevation;
        p
    }

    /// The planar `(u, v)` of a local point in the plane square to this
    /// axis.
    pub fn planar(self, p: [f64; 3]) -> [f64; 2] {
        let [i, j] = self.plane_axes();
        [p[i], p[j]]
    }
}

/// The axis square to the sketch plane: local Z.
fn sketch_normal() -> SpatialAxis {
    SpatialAxis::Z
}

fn is_sketch_normal(axis: &SpatialAxis) -> bool {
    *axis == SpatialAxis::Z
}

/// Which way an aligned line points along its axis, from its start to its
/// end (an aligned normal, a plane-bound normal likewise); whether two
/// tangent curves' tangents point the same way.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SpatialSense {
    #[default]
    Positive,
    Negative,
}

impl SpatialSense {
    /// 1 or -1.
    pub fn sign(self) -> f64 {
        match self {
            Self::Positive => 1.0,
            Self::Negative => -1.0,
        }
    }

    fn solver(self) -> AxisSense {
        match self {
            Self::Positive => AxisSense::Positive,
            Self::Negative => AxisSense::Negative,
        }
    }

    fn of_solver(sense: AxisSense) -> Self {
        match sense {
            AxisSense::Positive => Self::Positive,
            AxisSense::Negative => Self::Negative,
        }
    }
}

/// A frame axis with a sense: the constant normal of a plane-bound arc or
/// circle (`{"axis": "z", "sense": "positive"}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialDirection {
    pub axis: SpatialAxis,
    #[serde(default)]
    pub sense: SpatialSense,
}

impl SpatialDirection {
    /// The sketch plane's normal, local `+Z`: the plane of converted arcs
    /// and circles.
    pub const SKETCH_PLANE: Self = Self {
        axis: SpatialAxis::Z,
        sense: SpatialSense::Positive,
    };

    /// The unit vector.
    pub fn unit(self) -> [f64; 3] {
        let mut v = [0.0; 3];
        v[self.axis.index()] = self.sense.sign();
        v
    }

    fn solver(self) -> SpatialNormal {
        SpatialNormal::Axis(self.axis.solver(), self.sense.solver())
    }
}

/// A rotated placement plane's frame (mitcad#1661): the unit in-plane `x`
/// and the unit `normal` (`y` is `normal x x`), in the sketch's local
/// coordinates. A shape drawn on a rotated plane keeps it with its
/// relations ([`SpatialConstraintKind::InFrame`],
/// [`SpatialConstraintKind::FrameBound`]): the plane's directions, never
/// its place (the shape's points place it) and never the sketch's own
/// frame. Written `{"x": [1, 0, 0], "normal": [0, -1, 0]}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialFrame {
    pub x: [f64; 3],
    pub normal: [f64; 3],
}

impl SpatialFrame {
    /// The frame of an in-plane direction and a normal, made unit and
    /// square (`x` taken across `normal`); refused when they are not
    /// finite or have no length, or `x` lies along `normal`.
    pub fn new(x: [f64; 3], normal: [f64; 3]) -> Result<Self, String> {
        let refused = || {
            crate::said(
                "a rotated plane needs a direction in it and a normal across it",
                format!(
                    "a rotated plane needs an x axis in it and a normal across it, got x {x:?} \
                     and normal {normal:?}"
                ),
            )
        };
        if !finite3(x) || !finite3(normal) {
            return Err(refused());
        }
        let n = unit(normal).ok_or_else(refused)?;
        let x = unit(sub(x, n.map(|v| v * dot(x, n)))).ok_or_else(refused)?;
        Ok(Self { x, normal: n })
    }

    /// The plane's second in-plane axis, `normal x x`.
    pub fn y(&self) -> [f64; 3] {
        cross(self.normal, self.x)
    }

    /// The direction the frame gives an axis: its x, y or normal for X, Y
    /// or Z (an axis or offset typed on the rotated plane).
    pub fn axis(&self, axis: SpatialAxis) -> [f64; 3] {
        match axis {
            SpatialAxis::X => self.x,
            SpatialAxis::Y => self.y(),
            SpatialAxis::Z => self.normal,
        }
    }

    /// Whether the stored axes are unit and square (within 1e-9).
    pub fn is_valid(&self) -> bool {
        let unit_len = |v: [f64; 3]| (norm(v) - 1.0).abs() <= 1e-9;
        finite3(self.x)
            && finite3(self.normal)
            && unit_len(self.x)
            && unit_len(self.normal)
            && dot(self.x, self.normal).abs() <= 1e-9
    }

    /// A point at planar `(u, v)` and `elevation` along the normal.
    pub fn place(&self, uv: [f64; 2], elevation: f64) -> [f64; 3] {
        let y = self.y();
        let mut p = [0.0; 3];
        for (k, out) in p.iter_mut().enumerate() {
            *out = self.x[k] * uv[0] + y[k] * uv[1] + self.normal[k] * elevation;
        }
        p
    }

    /// The planar `(u, v)` of a local point: its coordinates along `x`
    /// and `y`.
    pub fn planar(&self, p: [f64; 3]) -> [f64; 2] {
        [dot(p, self.x), dot(p, self.y())]
    }

    /// A local point's coordinate along the normal.
    pub fn elevation(&self, p: [f64; 3]) -> f64 {
        dot(p, self.normal)
    }
}

/// The plane a plane-bound shape is drawn on: square to a frame axis
/// (mitcad#1550), or a rotated plane (mitcad#1661).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpatialPlane {
    Axis(SpatialAxis),
    Frame(SpatialFrame),
}

impl From<SpatialAxis> for SpatialPlane {
    fn from(axis: SpatialAxis) -> Self {
        Self::Axis(axis)
    }
}

impl SpatialPlane {
    /// A point at planar `(u, v)` and `elevation` along the normal.
    pub fn place(&self, uv: [f64; 2], elevation: f64) -> [f64; 3] {
        match self {
            Self::Axis(axis) => axis.place(uv, elevation),
            Self::Frame(frame) => frame.place(uv, elevation),
        }
    }

    /// The planar `(u, v)` of a local point.
    pub fn planar(&self, p: [f64; 3]) -> [f64; 2] {
        match self {
            Self::Axis(axis) => axis.planar(p),
            Self::Frame(frame) => frame.planar(p),
        }
    }

    /// A local point's coordinate along the normal.
    pub fn elevation(&self, p: [f64; 3]) -> f64 {
        match self {
            Self::Axis(axis) => p[axis.index()],
            Self::Frame(frame) => frame.elevation(p),
        }
    }

    /// The unit normal.
    pub fn normal(&self) -> [f64; 3] {
        match self {
            Self::Axis(axis) => axis.place([0.0, 0.0], 1.0),
            Self::Frame(frame) => frame.normal,
        }
    }

    /// The frame of a rotated plane; none for a frame axis' plane.
    pub fn frame(&self) -> Option<SpatialFrame> {
        match self {
            Self::Axis(_) => None,
            Self::Frame(frame) => Some(*frame),
        }
    }
}

/// An end condition of an open fit-point spline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialSplineEnd {
    /// No curvature at the end (dropped where a tangency takes the end,
    /// which leaves the tangent's magnitude free).
    #[default]
    Natural,
    /// A free end tangent (direction and magnitude).
    Free,
}

impl SpatialSplineEnd {
    fn solver(self) -> SplineEnd {
        match self {
            Self::Natural => SplineEnd::Natural,
            Self::Free => SplineEnd::Free,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn natural_ends(ends: &[SpatialSplineEnd; 2]) -> bool {
    *ends == [SpatialSplineEnd::Natural; 2]
}

fn no_tangents(tangents: &[Option<[f64; 3]>; 2]) -> bool {
    tangents.iter().all(Option::is_none)
}

/// A point: its id and local coordinates (millimetres), where the last
/// solve left it and where the next one starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialPoint {
    #[serde(with = "point_serde")]
    pub id: EntityUid,
    pub at: [f64; 3],
}

/// The geometry of a curve of a 3D sketch, by the points that define it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpatialCurveKind {
    /// A line from `start` to `end`.
    Line {
        #[serde(with = "point_serde")]
        start: EntityUid,
        #[serde(with = "point_serde")]
        end: EntityUid,
    },
    /// Counter-clockwise about `normal` from `start` to `end` around
    /// `center`, with a sweep strictly between nothing and a full turn.
    /// `normal` is the last solved unit normal (free: two degrees of
    /// freedom); `plane` binds it to a frame axis instead (a plane-bound
    /// arc: converted from a planar sketch, in a plane parallel to the
    /// sketch plane).
    Arc {
        #[serde(with = "point_serde")]
        center: EntityUid,
        #[serde(with = "point_serde")]
        start: EntityUid,
        #[serde(with = "point_serde")]
        end: EntityUid,
        normal: [f64; 3],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plane: Option<SpatialDirection>,
    },
    /// A circle around `center` with the last solved `radius` and unit
    /// `normal` (or bound to a frame axis by `plane`).
    Circle {
        #[serde(with = "point_serde")]
        center: EntityUid,
        radius: f64,
        normal: [f64; 3],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plane: Option<SpatialDirection>,
    },
    /// A B-spline by its control points (points of the sketch; it starts
    /// and ends at the first and last one with clamped knots). Empty
    /// `weights` is non-rational, empty `knots` a clamped uniform knot
    /// vector on [0, 1]; degree, weights and knots stay fixed.
    Spline {
        degree: u32,
        #[serde(with = "crate::points_serde")]
        control: Vec<EntityUid>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        weights: Vec<f64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        knots: Vec<f64>,
    },
    /// A cubic through fit points (chord-length parameters, as in planar
    /// sketches). Its control points are hidden unknowns of the solver,
    /// made from the fit points at every solve: they belong to it and go
    /// with it. `closed`: periodic through the first point (listed once,
    /// at least three); otherwise `ends` are its end conditions.
    FittedSpline {
        #[serde(with = "crate::points_serde")]
        points: Vec<EntityUid>,
        #[serde(default, skip_serializing_if = "is_false")]
        closed: bool,
        #[serde(default, skip_serializing_if = "natural_ends")]
        ends: [SpatialSplineEnd; 2],
        /// The end tangents `C'(t)` (the parameter running from 0 to 1)
        /// where the last solve left them, at an end that is free or held
        /// by a tangency; none at another end. The next solve starts from
        /// them, so a dragged tangent handle stays (mitcad#1526).
        #[serde(default, skip_serializing_if = "no_tangents")]
        tangents: [Option<[f64; 3]>; 2],
    },
    /// A plane-bound ellipse, kept from a converted planar sketch or drawn
    /// on a temporary plane (mitcad#1550): in the plane through its centre
    /// square to the frame axis `normal` (the sketch plane's Z by
    /// default), `major` the end point of its major axis. Its minor radius
    /// is solved like a circle's radius: stored where the last solve left
    /// it, driven by a dimension or a point on it.
    Ellipse {
        #[serde(with = "point_serde")]
        center: EntityUid,
        #[serde(with = "point_serde")]
        major: EntityUid,
        minor_radius: f64,
        #[serde(default = "sketch_normal", skip_serializing_if = "is_sketch_normal")]
        normal: SpatialAxis,
        /// Drawn on a rotated plane (mitcad#1661): in the plane through its
        /// centre square to the frame's normal, in place of `normal`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frame: Option<SpatialFrame>,
    },
    /// A plane-bound elliptical arc kept from a converted planar sketch,
    /// counter-clockwise about `normal` in the ellipse parameter from
    /// `start` to `end` (both kept on the ellipse).
    EllipticalArc {
        #[serde(with = "point_serde")]
        center: EntityUid,
        #[serde(with = "point_serde")]
        major: EntityUid,
        minor_radius: f64,
        #[serde(with = "point_serde")]
        start: EntityUid,
        #[serde(with = "point_serde")]
        end: EntityUid,
        #[serde(default = "sketch_normal", skip_serializing_if = "is_sketch_normal")]
        normal: SpatialAxis,
    },
    /// Constant geometry from outside the sketch (a source edge, another
    /// sketch's curve, an Intersection Curve: mitcad#1543), for points to
    /// lie on or at the middle of: the linked reference record of a curve,
    /// one per source and occurrence, reused by every point put on it. Its
    /// geometry in local coordinates is found before each solve
    /// ([`SpatialInputs::curves`]): in true 3D (Include), or with
    /// `projected` along the sketch's normal onto its plane (Project, the
    /// planar behaviour). It is never solved and no path piece.
    Reference {
        source: SpatialSource,
        #[serde(default, skip_serializing_if = "is_false")]
        projected: bool,
    },
}

/// A curve of a 3D sketch. A construction curve is no part of a route
/// unless it is picked explicitly. Written flat:
/// `{"id": "c3", "type": "line", "start": "p1", "end": "p2"}`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialCurve {
    pub id: EntityUid,
    pub kind: SpatialCurveKind,
    pub construction: bool,
}

impl SpatialCurve {
    pub fn new(id: EntityUid, kind: SpatialCurveKind, construction: bool) -> Self {
        Self {
            id,
            kind,
            construction,
        }
    }

    /// A line from `start` to `end`.
    pub fn line(id: EntityUid, start: EntityUid, end: EntityUid, construction: bool) -> Self {
        Self::new(id, SpatialCurveKind::Line { start, end }, construction)
    }

    /// The points that define it: a line's ends, an arc's centre, start
    /// and end, a circle's centre, a spline's control or fit points, an
    /// ellipse's centre and major point (and an elliptical arc's ends).
    pub fn points(&self) -> Vec<EntityUid> {
        use SpatialCurveKind::*;
        match &self.kind {
            Line { start, end } => vec![*start, *end],
            Arc {
                center, start, end, ..
            } => vec![*center, *start, *end],
            Circle { center, .. } => vec![*center],
            Spline { control, .. } => control.clone(),
            FittedSpline { points, .. } => points.clone(),
            Ellipse { center, major, .. } => vec![*center, *major],
            EllipticalArc {
                center,
                major,
                start,
                end,
                ..
            } => vec![*center, *major, *start, *end],
            Reference { .. } => Vec::new(),
        }
    }

    /// Its start and end points, where routes join it; None for a curve
    /// without ends (a circle, an ellipse, a closed spline, a reference).
    pub fn ends(&self) -> Option<[EntityUid; 2]> {
        use SpatialCurveKind::*;
        match &self.kind {
            Line { start, end } | Arc { start, end, .. } | EllipticalArc { start, end, .. } => {
                Some([*start, *end])
            }
            Spline { control: p, .. }
            | FittedSpline {
                points: p,
                closed: false,
                ..
            } => Some([*p.first()?, *p.last()?]),
            _ => None,
        }
    }

    /// The end points of a line; None for another kind.
    pub fn as_line(&self) -> Option<[EntityUid; 2]> {
        match self.kind {
            SpatialCurveKind::Line { start, end } => Some([start, end]),
            _ => None,
        }
    }

    /// An arc or a circle.
    pub fn is_round(&self) -> bool {
        matches!(
            self.kind,
            SpatialCurveKind::Arc { .. } | SpatialCurveKind::Circle { .. }
        )
    }

    /// A control-point or fit-point spline.
    pub fn is_spline(&self) -> bool {
        matches!(
            self.kind,
            SpatialCurveKind::Spline { .. } | SpatialCurveKind::FittedSpline { .. }
        )
    }

    /// Constant geometry from outside the sketch.
    pub fn is_reference(&self) -> bool {
        matches!(self.kind, SpatialCurveKind::Reference { .. })
    }

    /// Bound to a plane parallel to the sketch's (or to another frame
    /// plane): a plane-bound arc or circle, an ellipse.
    pub fn is_plane_bound(&self) -> bool {
        match &self.kind {
            SpatialCurveKind::Arc { plane, .. } | SpatialCurveKind::Circle { plane, .. } => {
                plane.is_some()
            }
            SpatialCurveKind::Ellipse { .. } | SpatialCurveKind::EllipticalArc { .. } => true,
            _ => false,
        }
    }

    /// The frame axis a plane-bound curve's plane is square to (an arc's
    /// or circle's bound normal, an ellipse's plane); None for another.
    pub fn plane_normal(&self) -> Option<SpatialAxis> {
        match &self.kind {
            SpatialCurveKind::Arc { plane, .. } | SpatialCurveKind::Circle { plane, .. } => {
                plane.map(|p| p.axis)
            }
            // On a rotated plane (mitcad#1661): square to no frame axis.
            SpatialCurveKind::Ellipse { frame: Some(_), .. } => None,
            SpatialCurveKind::Ellipse { normal, .. }
            | SpatialCurveKind::EllipticalArc { normal, .. } => Some(*normal),
            _ => None,
        }
    }

    /// The unit normal of a plane-bound ellipse's plane: its frame axis',
    /// or its rotated plane's (mitcad#1661); none for another curve.
    pub fn ellipse_normal(&self) -> Option<[f64; 3]> {
        match &self.kind {
            SpatialCurveKind::Ellipse {
                frame: Some(frame), ..
            } => Some(frame.normal),
            SpatialCurveKind::Ellipse { normal, .. }
            | SpatialCurveKind::EllipticalArc { normal, .. } => Some(normal.place([0.0, 0.0], 1.0)),
            _ => None,
        }
    }

    /// `line`, `arc`, `circle`, `spline`, `fitted_spline`, `ellipse`,
    /// `elliptical_arc` or `reference`.
    pub fn type_name(&self) -> &'static str {
        use SpatialCurveKind::*;
        match self.kind {
            Line { .. } => "line",
            Arc { .. } => "arc",
            Circle { .. } => "circle",
            Spline { .. } => "spline",
            FittedSpline { .. } => "fitted_spline",
            Ellipse { .. } => "ellipse",
            EllipticalArc { .. } => "elliptical_arc",
            Reference { .. } => "reference",
        }
    }
}

#[derive(Serialize)]
struct CurveOut<'a> {
    #[serde(with = "crate::ids::curve_serde")]
    id: EntityUid,
    #[serde(flatten)]
    kind: &'a SpatialCurveKind,
    #[serde(skip_serializing_if = "is_false")]
    construction: bool,
}

impl Serialize for SpatialCurve {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CurveOut {
            id: self.id,
            kind: &self.kind,
            construction: self.construction,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SpatialCurve {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = Map::deserialize(deserializer)?;
        let id = match map.remove("id") {
            Some(Value::String(text)) => EntityUid::parse_curve(&text).map_err(D::Error::custom)?,
            Some(_) => return Err(D::Error::custom("\"id\" must be a string")),
            None => return Err(D::Error::missing_field("id")),
        };
        let construction = match map.remove("construction") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => b,
            Some(_) => return Err(D::Error::custom("\"construction\" must be true or false")),
        };
        let kind = serde_json::from_value(Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Self {
            id,
            kind,
            construction,
        })
    }
}

/// A text kept from a converted planar sketch or placed by the Text tool
/// (mitcad#1550): its planar definition, in the plane square to the frame
/// axis `normal` (the sketch plane's Z by default) at the elevation `z`
/// along it; its anchor, angle and frame are in that plane's x and y
/// ([`SpatialAxis::plane_axes`]). It is not solved; a text in a frame names
/// points of the sketch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialText {
    pub planar: SketchText,
    #[serde(default)]
    pub z: f64,
    #[serde(default = "sketch_normal", skip_serializing_if = "is_sketch_normal")]
    pub normal: SpatialAxis,
    /// Placed on a rotated plane (mitcad#1661): in place of `normal`, its
    /// anchor, angle and frame in the frame's x and y, `z` along its
    /// normal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<SpatialFrame>,
}

impl SpatialText {
    /// The plane it lies on: square to `normal`, or its rotated plane
    /// (mitcad#1661).
    pub fn plane(&self) -> SpatialPlane {
        match self.frame {
            Some(frame) => SpatialPlane::Frame(frame),
            None => SpatialPlane::Axis(self.normal),
        }
    }
}

/// Which point of a source curve a point source is (mitcad#1543): its
/// start, its end, or its middle, halfway along it by length (the midpoint
/// the planar body-midpoint snap uses: a line's or an arc's middle). The
/// middle stays the middle when the source changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialCurvePoint {
    Start,
    End,
    Middle,
}

impl SpatialCurvePoint {
    /// `start`, `end` or `middle`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
            Self::Middle => "middle",
        }
    }
}

/// What a point is attached to, or where a reference curve comes from
/// (mitcad#1543): the sketch's origin; a vertex of a body; an edge of a
/// body (a curve, or with `at` its start, end or middle); a point or curve
/// of another sketch, planar or 3D (a curve, or with `at` a point of it);
/// the curve of an Intersection Curve (likewise). A body, sketch or
/// Intersection Curve of the sketch's own component comes before it in the
/// timeline; one of another component is seen through its occurrence
/// (`link`, as a planar projection's, mitcad#100), which follows that
/// component's changes and its placement. A source is a point
/// ([`SpatialSource::is_curve`] false: an attachment's), a curve (a
/// reference curve's) or a face of a body (an On Face relation's,
/// mitcad#1549: [`SpatialSource::is_face`]).
///
/// Written `"origin"` or an object that names its kind by its keys:
/// `{"body": "F2.b0", "vertex": "V{...}"}`, `{"body": "F2.b0", "edge":
/// "E{...}", "at": "middle"}`, `{"body": "F2.b0", "face": "F2:end(r0)"}`,
/// `{"sketch": "F3", "point": "p2"}`, `{"sketch": "F3", "curve": "c4",
/// "at": "end"}`, `{"feature": "F5"}`; each with `"link": {"source": "O1",
/// "target": ""}` for another component's geometry.
#[derive(Debug, Clone, PartialEq)]
pub enum SpatialSource {
    Origin,
    Vertex {
        body: BodyUid,
        vertex: VertexName,
        link: Option<OccurrenceLink>,
    },
    Edge {
        body: BodyUid,
        edge: Box<EdgeName>,
        at: Option<SpatialCurvePoint>,
        link: Option<OccurrenceLink>,
    },
    Sketch {
        entity: SketchEntityRef,
        at: Option<SpatialCurvePoint>,
        link: Option<OccurrenceLink>,
    },
    Curve {
        feature: FeatureUid,
        at: Option<SpatialCurvePoint>,
        link: Option<OccurrenceLink>,
    },
    Face {
        body: BodyUid,
        face: Box<FaceName>,
        link: Option<OccurrenceLink>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FaceSource {
    body: BodyUid,
    face: FaceName,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VertexSource {
    body: BodyUid,
    vertex: VertexName,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeSource {
    body: BodyUid,
    edge: EdgeName,
}

const SOURCE_FORMS: &str = "a source is \"origin\", {\"body\", \"vertex\"}, {\"body\", \"edge\"}, \
                            {\"body\", \"face\"}, {\"sketch\", \"point\" or \"curve\"} or \
                            {\"feature\"} (an Intersection Curve), with \"at\" (the start, end \
                            or middle of a curve) and \"link\" where they apply";

impl Serialize for SpatialSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        fn text(v: &dyn fmt::Display) -> Value {
            Value::String(v.to_string())
        }
        let mut map = Map::new();
        let (at, link) = match self {
            Self::Origin => return serializer.serialize_str("origin"),
            Self::Vertex { body, vertex, link } => {
                map.insert("body".to_owned(), text(body));
                map.insert("vertex".to_owned(), text(vertex));
                (None, link)
            }
            Self::Edge {
                body,
                edge,
                at,
                link,
            } => {
                map.insert("body".to_owned(), text(body));
                map.insert("edge".to_owned(), text(edge));
                (*at, link)
            }
            Self::Sketch { entity, at, link } => {
                if let Value::Object(entity) = entity.to_json() {
                    map.extend(entity);
                }
                (*at, link)
            }
            Self::Curve { feature, at, link } => {
                map.insert("feature".to_owned(), text(feature));
                (*at, link)
            }
            Self::Face { body, face, link } => {
                map.insert("body".to_owned(), text(body));
                map.insert("face".to_owned(), text(face));
                (None, link)
            }
        };
        if let Some(at) = at {
            map.insert("at".to_owned(), Value::String(at.name().to_owned()));
        }
        if let Some(link) = link {
            let link =
                serde_json::to_value(link).map_err(<S::Error as serde::ser::Error>::custom)?;
            map.insert("link".to_owned(), link);
        }
        Value::Object(map).serialize(serializer)
    }
}

/// Takes an optional field out of a source's object.
fn take_field<T: serde::de::DeserializeOwned>(
    map: &mut Map<String, Value>,
    key: &str,
) -> Result<Option<T>, String> {
    match map.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|e| format!("a source's \"{key}\": {e}")),
    }
}

impl SpatialSource {
    /// A source from its written form.
    pub fn from_json(value: Value) -> Result<Self, String> {
        let mut map = match value {
            Value::String(text) if text == "origin" => return Ok(Self::Origin),
            Value::Object(map) => map,
            _ => return Err(SOURCE_FORMS.to_owned()),
        };
        let link: Option<OccurrenceLink> = take_field(&mut map, "link")?;
        let at: Option<SpatialCurvePoint> = take_field(&mut map, "at")?;
        let object = Value::Object(map.clone());
        if map.contains_key("vertex") {
            if at.is_some() {
                return Err("a vertex is a point: it takes no \"at\"".to_owned());
            }
            let source: VertexSource = serde_json::from_value(object).map_err(|e| e.to_string())?;
            return Ok(Self::Vertex {
                body: source.body,
                vertex: source.vertex,
                link,
            });
        }
        if map.contains_key("face") {
            if at.is_some() {
                return Err("a face takes no \"at\"".to_owned());
            }
            let source: FaceSource = serde_json::from_value(object).map_err(|e| e.to_string())?;
            return Ok(Self::Face {
                body: source.body,
                face: Box::new(source.face),
                link,
            });
        }
        if map.contains_key("edge") {
            let source: EdgeSource = serde_json::from_value(object).map_err(|e| e.to_string())?;
            return Ok(Self::Edge {
                body: source.body,
                edge: Box::new(source.edge),
                at,
                link,
            });
        }
        if map.contains_key("sketch") {
            let entity = SketchEntityRef::from_json(&object)?;
            if entity.entity.is_point() && at.is_some() {
                return Err("a sketch's point takes no \"at\"".to_owned());
            }
            return Ok(Self::Sketch { entity, at, link });
        }
        if map.contains_key("feature") {
            if map.len() != 1 {
                return Err(SOURCE_FORMS.to_owned());
            }
            let feature = map["feature"]
                .as_str()
                .ok_or("\"feature\" is a feature's uid like \"F5\"")?
                .parse::<FeatureUid>()
                .map_err(|e| e.to_string())?;
            return Ok(Self::Curve { feature, at, link });
        }
        Err(SOURCE_FORMS.to_owned())
    }

    /// The features it names: a body's creator and the features of its
    /// vertex's or edge's faces, another sketch, an Intersection Curve.
    pub fn features(&self) -> BTreeSet<FeatureUid> {
        let mut features = BTreeSet::new();
        match self {
            Self::Origin => {}
            Self::Vertex { body, vertex, .. } => {
                features.extend(vertex.faces().iter().flat_map(|f| f.features()));
                features.insert(body.feature);
            }
            Self::Edge { body, edge, .. } => {
                features.extend(edge.features());
                features.insert(body.feature);
            }
            Self::Sketch { entity, .. } => {
                features.insert(entity.sketch);
            }
            Self::Curve { feature, .. } => {
                features.insert(*feature);
            }
            Self::Face { body, face, .. } => {
                features.extend(face.features());
                features.insert(body.feature);
            }
        }
        features
    }

    /// The body it is on (a vertex's, an edge's or a face's).
    pub fn body(&self) -> Option<BodyUid> {
        match self {
            Self::Vertex { body, .. } | Self::Edge { body, .. } | Self::Face { body, .. } => {
                Some(*body)
            }
            Self::Origin | Self::Sketch { .. } | Self::Curve { .. } => None,
        }
    }

    /// Where it is seen when it is geometry of another component.
    pub fn link(&self) -> Option<&OccurrenceLink> {
        match self {
            Self::Origin => None,
            Self::Vertex { link, .. }
            | Self::Edge { link, .. }
            | Self::Sketch { link, .. }
            | Self::Curve { link, .. }
            | Self::Face { link, .. } => link.as_ref(),
        }
    }

    /// Which point of a curve it is, when it is one.
    pub fn at(&self) -> Option<SpatialCurvePoint> {
        match self {
            Self::Edge { at, .. } | Self::Sketch { at, .. } | Self::Curve { at, .. } => *at,
            Self::Origin | Self::Vertex { .. } | Self::Face { .. } => None,
        }
    }

    /// A curve (a reference curve's source): an edge, another sketch's
    /// curve or an Intersection Curve, without `at`. The others are
    /// points (an attachment's source) or faces.
    pub fn is_curve(&self) -> bool {
        match self {
            Self::Origin | Self::Vertex { .. } | Self::Face { .. } => false,
            Self::Edge { at, .. } | Self::Curve { at, .. } => at.is_none(),
            Self::Sketch { entity, at, .. } => !entity.entity.is_point() && at.is_none(),
        }
    }

    /// A face of a body (an On Face relation's source, mitcad#1549).
    pub fn is_face(&self) -> bool {
        matches!(self, Self::Face { .. })
    }

    /// `point`, `curve` or `face` ([`SpatialSource::is_curve`],
    /// [`SpatialSource::is_face`]).
    pub fn kind_name(&self) -> &'static str {
        if self.is_face() {
            "face"
        } else if self.is_curve() {
            "curve"
        } else {
            "point"
        }
    }

    /// The vertex, edge or face of a body of the sketch's own component it
    /// is, as a message's place (mitcad#1644): painted while the pointer is
    /// over the message. None for the origin, another sketch's entity, an
    /// Intersection Curve and another component's geometry.
    pub fn place(&self) -> Option<NoticePlace> {
        if self.link().is_some() {
            return None;
        }
        let (place, body) = match self {
            Self::Vertex { body, vertex, .. } => (NoticePlace::vertex(vertex.to_string()), body),
            Self::Edge { body, edge, .. } => (NoticePlace::edge(edge.to_string()), body),
            Self::Face { body, face, .. } => (NoticePlace::face(face.to_string()), body),
            Self::Origin | Self::Sketch { .. } | Self::Curve { .. } => return None,
        };
        Some(place.in_body(&body.to_string()))
    }

    /// Its place, as a list ([`SpatialSource::place`]).
    pub fn places(&self) -> Vec<NoticePlace> {
        self.place().into_iter().collect()
    }
}

impl<'de> Deserialize<'de> for SpatialSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_json(Value::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl fmt::Display for SpatialSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(at) = self.at() {
            write!(f, "the {} of ", at.name())?;
        }
        match self {
            Self::Origin => f.write_str("the origin")?,
            Self::Vertex { body, vertex, .. } => write!(f, "vertex {vertex} of {body}")?,
            Self::Edge { body, edge, .. } => write!(f, "edge {edge} of {body}")?,
            Self::Sketch { entity, .. } => {
                let kind = if entity.entity.is_point() {
                    "point"
                } else {
                    "curve"
                };
                write!(f, "{kind} {} of {}", entity.entity, entity.sketch)?;
            }
            Self::Curve { feature, .. } => write!(f, "the curve of {feature}")?,
            Self::Face { body, face, .. } => write!(f, "face {face} of {body}")?,
        }
        if let Some(link) = self.link() {
            write!(f, " (seen at occurrence {})", link.source)?;
        }
        Ok(())
    }
}

/// A relation of a 3D sketch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpatialConstraintKind {
    /// Two separately made points at one place.
    Coincident {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
    },
    /// A point held at a deliberate local anchor (Fix; Unfix removes it and
    /// nothing else).
    Fix {
        #[serde(with = "point_serde")]
        point: EntityUid,
        at: [f64; 3],
    },
    /// A point attached to a source point (Detach removes it and nothing
    /// else): the linked reference record of a point (mitcad#1543), its
    /// target found before each solve, in true 3D (Include) or with
    /// `projected` along the sketch's normal onto its plane (Project).
    Attach {
        #[serde(with = "point_serde")]
        point: EntityUid,
        source: SpatialSource,
        #[serde(default, skip_serializing_if = "is_false")]
        projected: bool,
    },
    /// A line parallel to a frame axis, anywhere (not on the axis),
    /// pointing `sense` along it.
    Align {
        #[serde(with = "crate::ids::curve_serde")]
        line: EntityUid,
        axis: SpatialAxis,
        #[serde(default)]
        sense: SpatialSense,
    },
    /// A point on a curve that it does not define: a line between its
    /// ends, the full circle of an arc or circle, a spline or a reference
    /// curve within its range (a point at an end is on it).
    PointOn {
        #[serde(with = "point_serde")]
        point: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
    },
    /// A point at the middle of a line, of an arc (on the arc, not on its
    /// complement) or of a reference curve.
    Midpoint {
        #[serde(with = "point_serde")]
        point: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
    },
    /// Two curves tangent at the point `at` (an end of each arc and spline,
    /// or a point on an arc or circle; a line needs not pass through it).
    /// `sense` says whether their tangents point the same way; it is taken
    /// from the geometry when the relation is made, then kept (a solve
    /// that would turn it is refused).
    Tangent {
        #[serde(with = "crate::ids::curve_serde")]
        a: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        b: EntityUid,
        #[serde(with = "point_serde")]
        at: EntityUid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sense: Option<SpatialSense>,
    },
    /// Equal lengths of two lines, or equal radii of two arcs or circles.
    Equal {
        #[serde(with = "crate::ids::curve_serde")]
        a: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        b: EntityUid,
    },
    /// An arc's or circle's free normal along a frame axis, pointing
    /// `sense` along it.
    NormalAlign {
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
        axis: SpatialAxis,
        #[serde(default)]
        sense: SpatialSense,
    },
    /// Points on the sketch plane (local `z = 0`): one representative of
    /// each group of points whose elevations are tied (by coincidences and
    /// plane-bound curves), so the others follow it
    /// ([`SpatialSketchDef::elevation_representatives`]).
    OnSketchPlane {
        #[serde(with = "crate::points_serde")]
        points: Vec<EntityUid>,
    },
    /// A planar constraint kept by a sketch converted from a planar one,
    /// solved on the x and y of its points (its curves: lines, arcs and
    /// circles in the sketch plane, splines); or made by a plane-bound
    /// drawing tool (mitcad#1550: a rectangle's, a polygon's) in the plane
    /// square to the frame axis `normal`, solved on that plane's two
    /// coordinates ([`SpatialAxis::plane_axes`]).
    InPlane {
        relation: ConstraintKind,
        #[serde(default = "sketch_normal", skip_serializing_if = "is_sketch_normal")]
        normal: SpatialAxis,
    },
    /// A planar fixed entity kept by a converted sketch: its x and y stay
    /// where they are (its elevation does not).
    InPlaneFixed { entity: Ref },
    /// A point on a face of a body (mitcad#1549; Detach removes it and
    /// nothing else): the linked reference record of a face, its surface
    /// found before each solve. The point keeps two degrees of freedom to
    /// slide on it: on the whole surface of a plane, cylinder, cone, sphere
    /// or torus, on the face itself for another kind of surface (through
    /// the kernel's closest-point projection).
    OnFace {
        #[serde(with = "point_serde")]
        point: EntityUid,
        source: SpatialSource,
    },
    /// Points kept in one plane square to the frame axis `normal`
    /// (mitcad#1550: a plane-bound shape drawn on a temporary plane): their
    /// coordinates along it stay equal, wherever that plane is.
    PlaneBound {
        #[serde(with = "crate::points_serde")]
        points: Vec<EntityUid>,
        normal: SpatialAxis,
    },
    /// Two directions parallel, either way along each other, with no
    /// intersection implied (mitcad#1661): each a line's or the normal of
    /// an arc or circle.
    Parallel {
        #[serde(with = "crate::ids::curve_serde")]
        a: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        b: EntityUid,
    },
    /// Two directions at right angles (lines' or arcs' and circles'
    /// normals), with no intersection implied (mitcad#1661).
    Perpendicular {
        #[serde(with = "crate::ids::curve_serde")]
        a: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        b: EntityUid,
    },
    /// Two points mirrored across a plane (mitcad#1661): a plane of the
    /// sketch's frame, a construction plane, a planar face or another
    /// sketch's plane ([`SpatialPlaneRef`]).
    Symmetric {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
        plane: SpatialPlaneRef,
    },
    /// Two arcs or circles with one centre and, with `normal` (the
    /// default), in parallel planes; without it the centre only
    /// (mitcad#1661).
    Concentric {
        #[serde(with = "crate::ids::curve_serde")]
        a: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        b: EntityUid,
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        normal: bool,
    },
    /// Curvature continuity (G2) at the point `at` where a spline ends and
    /// a line, arc or spline joins it (mitcad#1661): tangent there (with
    /// `sense` as a tangency's) and with equal curvature.
    Curvature {
        #[serde(with = "crate::ids::curve_serde")]
        a: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        b: EntityUid,
        #[serde(with = "point_serde")]
        at: EntityUid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sense: Option<SpatialSense>,
    },
    /// Points on a plane other than the sketch plane (mitcad#1661): a
    /// plane of the sketch's frame, a construction plane, a planar face or
    /// another sketch's plane; each point keeps two degrees of freedom on
    /// it.
    OnPlane {
        #[serde(with = "crate::points_serde")]
        points: Vec<EntityUid>,
        plane: SpatialPlaneRef,
    },
    /// A planar relation of a shape drawn on a rotated placement plane
    /// (mitcad#1661: a rectangle's, a polygon's), solved on its points'
    /// coordinates along the frame's x and y as
    /// [`SpatialConstraintKind::InPlane`] is on a frame plane's.
    InFrame {
        relation: ConstraintKind,
        frame: SpatialFrame,
    },
    /// A shape's points kept in one plane square to a rotated plane's
    /// normal (mitcad#1661): their coordinates along it stay equal,
    /// wherever that plane is.
    FrameBound {
        #[serde(with = "crate::points_serde")]
        points: Vec<EntityUid>,
        frame: SpatialFrame,
    },
    /// A line along a unit direction, anywhere, pointing `sense` along it
    /// (mitcad#1661: Keep Axis on a rotated placement plane).
    AlignAlong {
        #[serde(with = "crate::ids::curve_serde")]
        line: EntityUid,
        direction: [f64; 3],
        #[serde(default)]
        sense: SpatialSense,
    },
    /// An arc's or circle's free normal along a unit direction, either way
    /// (mitcad#1661: a circle drawn on a rotated placement plane).
    NormalAlong {
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
        direction: [f64; 3],
    },
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// A plane a relation of a 3D sketch takes (mitcad#1661: Symmetric, On
/// Plane). Written `{"frame": "z"}` (the plane of the sketch's frame square
/// to the axis, through its origin: `z` is the sketch plane, `x` the YZ
/// and `y` the ZX plane), `{"construction": "F3"}` (a construction plane
/// of the sketch's component), `{"face": {"body": "F2.b0", "face":
/// "F2:top"}}` (a planar face of a body, of another component through a
/// `link`) or `{"sketch": "F4"}` (another sketch's plane: a planar
/// sketch's, or a 3D sketch's XY plane).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialPlaneRef {
    Frame(SpatialAxis),
    Construction(FeatureUid),
    Face(SpatialSource),
    Sketch(FeatureUid),
}

impl SpatialPlaneRef {
    /// The features it names.
    pub fn features(&self) -> BTreeSet<FeatureUid> {
        match self {
            Self::Frame(_) => BTreeSet::new(),
            Self::Construction(uid) | Self::Sketch(uid) => BTreeSet::from([*uid]),
            Self::Face(source) => source.features(),
        }
    }

    /// Whether it is found outside the sketch (everything but a frame
    /// plane).
    pub fn is_outside(&self) -> bool {
        !matches!(self, Self::Frame(_))
    }

    /// Where it is, for a person (mitcad#1327: painted, never named).
    pub fn places(&self) -> Vec<NoticePlace> {
        match self {
            Self::Face(source) => source.places(),
            _ => Vec::new(),
        }
    }

    /// A plane of the frame: through the origin, square to its axis.
    pub fn frame_surface(axis: SpatialAxis) -> SpatialSurface {
        let mut normal = [0.0; 3];
        normal[axis.index()] = 1.0;
        SpatialSurface::Plane {
            origin: [0.0; 3],
            normal,
        }
    }
}

impl fmt::Display for SpatialPlaneRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame(axis) => write!(f, "the frame plane {}", axis.plane_name()),
            Self::Construction(uid) => write!(f, "construction plane {uid}"),
            Self::Face(source) => write!(f, "{source}"),
            Self::Sketch(uid) => write!(f, "the plane of sketch {uid}"),
        }
    }
}

/// The start or the end of an open curve (mitcad#1661: a spline's tangent
/// magnitude).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialCurveEnd {
    Start,
    End,
}

impl SpatialCurveEnd {
    /// 0 for the start, 1 for the end.
    pub fn index(self) -> usize {
        match self {
            Self::Start => 0,
            Self::End => 1,
        }
    }
}

impl SpatialConstraintKind {
    /// The points it names.
    pub fn points(&self) -> Vec<EntityUid> {
        use SpatialConstraintKind::*;
        match self {
            Coincident { a, b } => vec![*a, *b],
            Fix { point, .. }
            | Attach { point, .. }
            | PointOn { point, .. }
            | Midpoint { point, .. }
            | OnFace { point, .. } => vec![*point],
            Tangent { at, .. } | Curvature { at, .. } => vec![*at],
            OnSketchPlane { points } | PlaneBound { points, .. } | OnPlane { points, .. } => {
                points.clone()
            }
            Symmetric { a, b, .. } => vec![*a, *b],
            InPlane { relation, .. } => refs_of(relation.refs(), true),
            InPlaneFixed { entity } => refs_of(vec![*entity], true),
            InFrame { relation, .. } => refs_of(relation.refs(), true),
            FrameBound { points, .. } => points.clone(),
            Align { .. }
            | Equal { .. }
            | NormalAlign { .. }
            | Parallel { .. }
            | Perpendicular { .. }
            | AlignAlong { .. }
            | NormalAlong { .. }
            | Concentric { .. } => Vec::new(),
        }
    }

    /// The curves it names.
    pub fn curves(&self) -> Vec<EntityUid> {
        use SpatialConstraintKind::*;
        match self {
            Align { line, .. } => vec![*line],
            PointOn { curve, .. } | Midpoint { curve, .. } | NormalAlign { curve, .. } => {
                vec![*curve]
            }
            Tangent { a, b, .. }
            | Curvature { a, b, .. }
            | Equal { a, b }
            | Parallel { a, b }
            | Perpendicular { a, b }
            | Concentric { a, b, .. } => vec![*a, *b],
            InPlane { relation, .. } => refs_of(relation.refs(), false),
            InPlaneFixed { entity } => refs_of(vec![*entity], false),
            InFrame { relation, .. } => refs_of(relation.refs(), false),
            AlignAlong { line, .. } => vec![*line],
            NormalAlong { curve, .. } => vec![*curve],
            FrameBound { .. } => Vec::new(),
            Coincident { .. }
            | Fix { .. }
            | Attach { .. }
            | OnSketchPlane { .. }
            | OnFace { .. }
            | Symmetric { .. }
            | OnPlane { .. }
            | PlaneBound { .. } => Vec::new(),
        }
    }

    /// The line it names (an alignment's).
    pub fn line(&self) -> Option<EntityUid> {
        match self {
            Self::Align { line, .. } | Self::AlignAlong { line, .. } => Some(*line),
            _ => None,
        }
    }

    /// An in-plane relation (kept from a planar sketch, or of a shape drawn
    /// on a plane: a frame plane, a rotated one).
    pub fn is_in_plane(&self) -> bool {
        matches!(
            self,
            Self::InPlane { .. } | Self::InPlaneFixed { .. } | Self::InFrame { .. }
        )
    }

    /// The frame axis an in-plane relation's plane is square to (Z for a
    /// converted sketch's).
    pub fn in_plane_normal(&self) -> Option<SpatialAxis> {
        match self {
            Self::InPlane { normal, .. } => Some(*normal),
            Self::InPlaneFixed { .. } => Some(SpatialAxis::Z),
            _ => None,
        }
    }

    /// `coincident`, `fix`, `attach`, `align`, `point_on`, `midpoint`,
    /// `tangent`, `equal`, `normal_align`, `on_sketch_plane`, `in_plane`,
    /// `in_plane_fixed`, `on_face`, `plane_bound`, `parallel`,
    /// `perpendicular`, `symmetric`, `concentric`, `curvature` or
    /// `on_plane`.
    pub fn type_name(&self) -> &'static str {
        use SpatialConstraintKind::*;
        match self {
            Coincident { .. } => "coincident",
            Fix { .. } => "fix",
            Attach { .. } => "attach",
            Align { .. } => "align",
            PointOn { .. } => "point_on",
            Midpoint { .. } => "midpoint",
            Tangent { .. } => "tangent",
            Equal { .. } => "equal",
            NormalAlign { .. } => "normal_align",
            OnSketchPlane { .. } => "on_sketch_plane",
            InPlane { .. } => "in_plane",
            InPlaneFixed { .. } => "in_plane_fixed",
            OnFace { .. } => "on_face",
            PlaneBound { .. } => "plane_bound",
            Parallel { .. } => "parallel",
            Perpendicular { .. } => "perpendicular",
            Symmetric { .. } => "symmetric",
            Concentric { .. } => "concentric",
            Curvature { .. } => "curvature",
            OnPlane { .. } => "on_plane",
            InFrame { .. } => "in_frame",
            FrameBound { .. } => "frame_bound",
            AlignAlong { .. } => "align_along",
            NormalAlong { .. } => "normal_along",
        }
    }

    /// The plane a Symmetric or On Plane relation takes (mitcad#1661).
    pub fn plane(&self) -> Option<&SpatialPlaneRef> {
        match self {
            Self::Symmetric { plane, .. } | Self::OnPlane { plane, .. } => Some(plane),
            _ => None,
        }
    }
}

/// The points (`points`) or the curves of planar references.
fn refs_of(refs: Vec<Ref>, points: bool) -> Vec<EntityUid> {
    refs.into_iter()
        .filter(|r| r.is_point() == points)
        .map(Ref::uid)
        .collect()
}

/// A relation with its id, written flat:
/// `{"id": "k3", "type": "align", "line": "c2", "axis": "x"}`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialConstraintDef {
    pub id: ConstraintUid,
    pub kind: SpatialConstraintKind,
}

#[derive(Serialize)]
struct ConstraintOut<'a> {
    id: ConstraintUid,
    #[serde(flatten)]
    kind: &'a SpatialConstraintKind,
}

impl Serialize for SpatialConstraintDef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ConstraintOut {
            id: self.id,
            kind: &self.kind,
        }
        .serialize(serializer)
    }
}

fn take_id<E: serde::de::Error>(map: &mut Map<String, Value>) -> Result<ConstraintUid, E> {
    match map.remove("id") {
        Some(Value::String(text)) => text.parse().map_err(E::custom),
        Some(_) => Err(E::custom("\"id\" must be a string")),
        None => Err(E::missing_field("id")),
    }
}

impl<'de> Deserialize<'de> for SpatialConstraintDef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = Map::deserialize(deserializer)?;
        let id = take_id(&mut map)?;
        let kind = serde_json::from_value(Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Self { id, kind })
    }
}

/// What a dimension of a 3D sketch measures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpatialDimensionKind {
    /// The true length of a line (greater than zero).
    Length {
        #[serde(with = "crate::ids::curve_serde")]
        line: EntityUid,
    },
    /// The signed difference `b - a` along a frame axis (zero or negative
    /// too).
    Offset {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
        axis: SpatialAxis,
    },
    /// The radius of an arc or circle.
    Radius {
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
    },
    /// The diameter of an arc or circle.
    Diameter {
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
    },
    /// The length along an arc.
    ArcLength {
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
    },
    /// The length of a spline: driving (mitcad#1661) or driven.
    SplineLength {
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
    },
    /// A planar dimension kept by a sketch converted from a planar one,
    /// measured on the x and y of its points; or made with a plane-bound
    /// shape (mitcad#1550) in the plane square to the frame axis `normal`.
    InPlane {
        dimension: DimensionKind,
        #[serde(default = "sketch_normal", skip_serializing_if = "is_sketch_normal")]
        normal: SpatialAxis,
    },
    /// The major radius of an ellipse: from its centre to its major point
    /// (mitcad#1550).
    MajorRadius {
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
    },
    /// The minor radius of an ellipse, which the solve finds (mitcad#1550).
    MinorRadius {
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
    },
    /// The angle between two directions (mitcad#1661): lines' (from start
    /// to end) or arcs' and circles' normals, strictly between nothing and
    /// a half turn (radians), with no intersection implied.
    Angle {
        #[serde(with = "crate::ids::curve_serde")]
        a: EntityUid,
        #[serde(with = "crate::ids::curve_serde")]
        b: EntityUid,
    },
    /// The magnitude of an open spline's end tangent `abs(C'(t))` at its
    /// start or end (mitcad#1661), in its parameter (millimetres: a
    /// fit-point spline's parameter runs from 0 to 1; its tangent handle is
    /// drawn a third of it long). The end must be free or held by a
    /// tangency or curvature continuity.
    TangentMagnitude {
        #[serde(with = "crate::ids::curve_serde")]
        curve: EntityUid,
        end: SpatialCurveEnd,
    },
    /// The signed difference `(b - a).direction` along a unit direction,
    /// zero or negative too (mitcad#1661: an offset typed along an axis of
    /// a rotated placement plane).
    OffsetAlong {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
        direction: [f64; 3],
    },
}

impl SpatialDimensionKind {
    /// `length`, `offset`, `radius`, `diameter`, `arc_length`,
    /// `spline_length`, `in_plane`, `major_radius`, `minor_radius`, `angle`
    /// or `tangent_magnitude`: the field of the value's slot (an angle's in
    /// degrees for a person).
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Length { .. } => "length",
            Self::Offset { .. } => "offset",
            Self::Radius { .. } => "radius",
            Self::Diameter { .. } => "diameter",
            Self::ArcLength { .. } => "arc_length",
            Self::SplineLength { .. } => "spline_length",
            Self::InPlane { .. } => "in_plane",
            Self::MajorRadius { .. } => "major_radius",
            Self::MinorRadius { .. } => "minor_radius",
            Self::Angle { .. } => "angle",
            Self::TangentMagnitude { .. } => "tangent_magnitude",
            Self::OffsetAlong { .. } => "offset_along",
        }
    }

    /// The points it names.
    pub fn points(&self) -> Vec<EntityUid> {
        match self {
            Self::Offset { a, b, .. } | Self::OffsetAlong { a, b, .. } => vec![*a, *b],
            Self::InPlane { dimension, .. } => refs_of(dimension.refs(), true),
            _ => Vec::new(),
        }
    }

    /// The curves it names.
    pub fn curves(&self) -> Vec<EntityUid> {
        match self {
            Self::Length { line: curve }
            | Self::Radius { curve }
            | Self::Diameter { curve }
            | Self::ArcLength { curve }
            | Self::MajorRadius { curve }
            | Self::MinorRadius { curve }
            | Self::TangentMagnitude { curve, .. }
            | Self::SplineLength { curve } => vec![*curve],
            Self::Angle { a, b } => vec![*a, *b],
            Self::InPlane { dimension, .. } => refs_of(dimension.refs(), false),
            Self::Offset { .. } | Self::OffsetAlong { .. } => Vec::new(),
        }
    }

    /// The line it names (a length's).
    pub fn line(&self) -> Option<EntityUid> {
        match self {
            Self::Length { line } => Some(*line),
            _ => None,
        }
    }
}

/// A dimension: driving with a parameter as its value, or driven (only
/// measured). Written flat: `{"id": "k5", "type": "length", "line": "c2",
/// "value": "d1"}`; without `value` it is driven.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialDimension<P> {
    pub id: ConstraintUid,
    pub kind: SpatialDimensionKind,
    /// The parameter of a driving dimension; None for a driven one.
    pub value: Option<P>,
    /// The read-only reference parameter of a driven dimension: the value
    /// it measures (mitcad#1661, as a planar sketch's, mitcad#231), which
    /// the features after the sketch and the sketch's own driving
    /// dimensions may use. None for a driving one, and for a driven one
    /// until the document gives it one.
    pub reference: Option<P>,
}

impl<P> SpatialDimension<P> {
    pub fn new(id: ConstraintUid, kind: SpatialDimensionKind, value: Option<P>) -> Self {
        Self {
            id,
            kind,
            value,
            reference: None,
        }
    }

    pub fn is_driven(&self) -> bool {
        self.value.is_none()
    }

    /// The slot of its value: `dimensions[k5].length`; an in-plane
    /// dimension's is its planar one's (`dimensions[k5].angle`), so the
    /// value keeps its unit (mitcad#1544).
    pub fn slot(&self) -> String {
        let field = match &self.kind {
            SpatialDimensionKind::InPlane { dimension, .. } => dimension.type_name(),
            kind => kind.type_name(),
        };
        format!("dimensions[{}].{field}", self.id)
    }

    pub fn map_value<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SpatialDimension<Q>, E> {
        Ok(SpatialDimension {
            id: self.id,
            kind: self.kind.clone(),
            value: match &self.value {
                Some(value) => Some(f(&self.slot(), value)?),
                None => None,
            },
            reference: match &self.reference {
                Some(reference) => Some(f(&self.slot(), reference)?),
                None => None,
            },
        })
    }
}

#[derive(Serialize)]
struct DimensionOut<'a, P> {
    id: ConstraintUid,
    #[serde(flatten)]
    kind: &'a SpatialDimensionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a P>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference: Option<&'a P>,
}

impl<P: Serialize> Serialize for SpatialDimension<P> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        DimensionOut {
            id: self.id,
            kind: &self.kind,
            value: self.value.as_ref(),
            reference: self.reference.as_ref(),
        }
        .serialize(serializer)
    }
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for SpatialDimension<P> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = Map::deserialize(deserializer)?;
        let id = take_id(&mut map)?;
        let value = match map.remove("value") {
            None | Some(Value::Null) => None,
            Some(value) => Some(P::deserialize(value).map_err(D::Error::custom)?),
        };
        let reference = match map.remove("reference") {
            None | Some(Value::Null) => None,
            Some(reference) => Some(P::deserialize(reference).map_err(D::Error::custom)?),
        };
        if value.is_some() && reference.is_some() {
            return Err(D::Error::custom(format!(
                "{id}: a driving dimension has no \"reference\""
            )));
        }
        let kind = serde_json::from_value(Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Self {
            id,
            kind,
            value,
            reference,
        })
    }
}

/// The stored definition of a 3D sketch (`spatial_sketch`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialSketchDef<P = ParamId> {
    /// What the frame follows: an origin plane, a construction plane or a
    /// planar face of the sketch's component.
    #[serde(default)]
    pub support: SketchPlane,
    /// The sketch's own frame in its support's frame (the support's x, y
    /// and normal), as a planar sketch's own frame (`FrameDef`): kept by a
    /// sketch converted from a planar one (mitcad#1544), whose x and y
    /// axes become the 3D sketch's. None: the support's frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<Box<FrameDef>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<SpatialPoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curves: Vec<SpatialCurve>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<SpatialConstraintDef>,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<SpatialDimension<P>>,
    /// Texts kept from a converted planar sketch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<SpatialText>,
    /// What a converted planar sketch keeps that 3D sketches have no own
    /// form for, acting in the sketch plane (mitcad#1550,
    /// [`crate::spatial_planar`]).
    #[serde(
        default = "PlanarRecords::default",
        skip_serializing_if = "PlanarRecords::is_empty"
    )]
    pub planar: PlanarRecords<P>,
}

impl<P> Default for SpatialSketchDef<P> {
    fn default() -> Self {
        Self {
            support: SketchPlane::Xy,
            frame: None,
            points: Vec::new(),
            curves: Vec::new(),
            constraints: Vec::new(),
            dimensions: Vec::new(),
            texts: Vec::new(),
            planar: PlanarRecords::default(),
        }
    }
}

fn finite3(v: [f64; 3]) -> bool {
    v.iter().all(|x| x.is_finite())
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = norm(v);
    (n > 1e-12 && n.is_finite()).then(|| [v[0] / n, v[1] / n, v[2] / n])
}

/// The sweep from `a` to `b` counter-clockwise about `n` (radians, in
/// `(0, 2 pi]`).
pub fn sweep_about(n: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let theta = dot(n, cross(a, b)).atan2(dot(a, b));
    if theta <= 0.0 { theta + TAU } else { theta }
}

/// A point's local coordinates, an arc's or circle's normal and radius:
/// what measuring needs, from the stored definition or a solve.
struct Geometry<'a> {
    curves: &'a [SpatialCurve],
    at: &'a dyn Fn(EntityUid) -> Option<[f64; 3]>,
    /// An arc's or circle's unit normal and radius.
    round: &'a dyn Fn(EntityUid) -> Option<([f64; 3], f64)>,
    /// An ellipse's minor radius.
    minor: &'a dyn Fn(EntityUid) -> Option<f64>,
}

impl Geometry<'_> {
    fn curve(&self, id: EntityUid) -> Option<&SpatialCurve> {
        self.curves.iter().find(|c| c.id == id)
    }

    /// The planar coordinates (in the plane square to `normal`) of the
    /// points and of the lines, arcs and circles about `normal` and the
    /// ellipses in its planes: the planar view in-plane dimensions measure.
    fn planar(&self, normal: SpatialAxis) -> Solved {
        let mut solved = Solved::default();
        for curve in self.curves {
            for p in curve.points() {
                if let Some(at) = self.xy(p, normal) {
                    solved.points.insert(p, at);
                }
            }
            if let Some(planar) = self.planar_curve(curve, normal) {
                solved.curves.insert(curve.id, planar);
            }
        }
        solved
    }

    fn xy(&self, id: EntityUid, normal: SpatialAxis) -> Option<[f64; 2]> {
        (self.at)(id).map(|p| normal.planar(p))
    }

    /// A line, an arc or circle about `normal` or an ellipse in the plane
    /// square to it, in the planar view.
    fn planar_curve(&self, curve: &SpatialCurve, along: SpatialAxis) -> Option<Curve2> {
        let xy = |id: EntityUid| self.xy(id, along);
        let angle = |c: [f64; 2], p: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]);
        Some(match &curve.kind {
            SpatialCurveKind::Line { start, end } => Curve2::Line {
                a: xy(*start)?,
                b: xy(*end)?,
            },
            SpatialCurveKind::Arc {
                center, start, end, ..
            } => {
                let (normal, radius) = (self.round)(curve.id)?;
                if normal[along.index()] <= 0.0 {
                    return None;
                }
                let (c, s, e) = (xy(*center)?, xy(*start)?, xy(*end)?);
                let a0 = angle(c, s);
                let mut a1 = angle(c, e);
                while a1 <= a0 {
                    a1 += TAU;
                }
                Curve2::Arc {
                    center: c,
                    radius,
                    start: a0,
                    end: a1,
                }
            }
            SpatialCurveKind::Circle { center, .. } => {
                let (normal, radius) = (self.round)(curve.id)?;
                if normal[along.index()].abs() < 1.0 - 1e-9 {
                    return None;
                }
                Curve2::Circle {
                    center: xy(*center)?,
                    radius,
                    start: 0.0,
                }
            }
            SpatialCurveKind::Ellipse {
                center,
                major,
                normal,
                frame: None,
                ..
            } if *normal == along => {
                let (c, m) = (xy(*center)?, xy(*major)?);
                Curve2::Ellipse {
                    center: c,
                    major: (m[0] - c[0]).hypot(m[1] - c[1]),
                    minor: (self.minor)(curve.id)?,
                    rotation: angle(c, m),
                }
            }
            _ => return None,
        })
    }

    /// The unit direction of a line (from its start to its end) or the
    /// unit normal of an arc or circle (mitcad#1661).
    fn direction(&self, id: EntityUid) -> Option<[f64; 3]> {
        let curve = self.curve(id)?;
        match curve.as_line() {
            Some([a, b]) => unit(sub((self.at)(b)?, (self.at)(a)?)),
            None if curve.is_round() => Some((self.round)(id)?.0),
            None => None,
        }
    }

    /// What a dimension measures: a line's true length, a signed offset, a
    /// radius, a diameter, an arc's length, an in-plane dimension on the
    /// planar view (`in_plane`: the sketch's in-plane relations, for a
    /// spacing). A spline's length is measured by the solver.
    fn measure(&self, kind: &SpatialDimensionKind, in_plane: &[Constraint]) -> Option<f64> {
        let at = self.at;
        match kind {
            SpatialDimensionKind::Length { line } => {
                let [a, b] = self.curve(*line)?.as_line()?;
                Some(norm(sub(at(b)?, at(a)?)))
            }
            SpatialDimensionKind::Offset { a, b, axis } => {
                Some(at(*b)?[axis.index()] - at(*a)?[axis.index()])
            }
            SpatialDimensionKind::OffsetAlong { a, b, direction } => {
                Some(dot(sub(at(*b)?, at(*a)?), *direction))
            }
            SpatialDimensionKind::Radius { curve } => Some((self.round)(*curve)?.1),
            SpatialDimensionKind::Diameter { curve } => Some(2.0 * (self.round)(*curve)?.1),
            SpatialDimensionKind::ArcLength { curve } => {
                let SpatialCurveKind::Arc {
                    center, start, end, ..
                } = self.curve(*curve)?.kind
                else {
                    return None;
                };
                let (normal, radius) = (self.round)(*curve)?;
                let c = at(center)?;
                Some(radius * sweep_about(normal, sub(at(start)?, c), sub(at(end)?, c)))
            }
            SpatialDimensionKind::SplineLength { .. }
            | SpatialDimensionKind::TangentMagnitude { .. } => None,
            SpatialDimensionKind::Angle { a, b } => {
                let (u, v) = (self.direction(*a)?, self.direction(*b)?);
                Some(norm(cross(u, v)).atan2(dot(u, v)))
            }
            SpatialDimensionKind::MajorRadius { curve } => {
                let (SpatialCurveKind::Ellipse { center, major, .. }
                | SpatialCurveKind::EllipticalArc { center, major, .. }) = self.curve(*curve)?.kind
                else {
                    return None;
                };
                Some(norm(sub(at(major)?, at(center)?)))
            }
            SpatialDimensionKind::MinorRadius { curve } => (self.minor)(*curve),
            SpatialDimensionKind::InPlane { dimension, normal } => {
                let mut view = self.planar(*normal);
                // Points on no curve too.
                let named = dimension
                    .refs()
                    .into_iter()
                    .chain(in_plane.iter().flat_map(|c| c.kind.refs()));
                for r in named {
                    if let Ref::Point(p) = r
                        && let Some(xy) = self.xy(p, *normal)
                    {
                        view.points.insert(p, xy);
                    }
                }
                measure_in(dimension, &view, in_plane)
            }
        }
    }
}

impl<P> SpatialSketchDef<P> {
    pub const TYPE: &'static str = "spatial_sketch";
    pub const BASE_NAME: &'static str = "3DSketch";

    /// An empty 3D sketch on `support`.
    pub fn new(support: SketchPlane) -> Self {
        Self {
            support,
            ..Self::default()
        }
    }

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SpatialSketchDef<Q>, E> {
        Ok(SpatialSketchDef {
            support: self.support.clone(),
            frame: self.frame.clone(),
            points: self.points.clone(),
            curves: self.curves.clone(),
            constraints: self.constraints.clone(),
            dimensions: self
                .dimensions
                .iter()
                .map(|d| d.map_value(f))
                .collect::<Result<_, _>>()?,
            texts: self.texts.clone(),
            planar: self.planar.map_params(f)?,
        })
    }

    pub fn point(&self, id: EntityUid) -> Option<&SpatialPoint> {
        self.points.iter().find(|p| p.id == id)
    }

    pub fn curve(&self, id: EntityUid) -> Option<&SpatialCurve> {
        self.curves.iter().find(|c| c.id == id)
    }

    /// The start and end points of a line (of a curve that is a line).
    pub fn line(&self, id: EntityUid) -> Option<[EntityUid; 2]> {
        self.curve(id)?.as_line()
    }

    pub fn constraint(&self, id: ConstraintUid) -> Option<&SpatialConstraintDef> {
        self.constraints.iter().find(|c| c.id == id)
    }

    pub fn dimension(&self, id: ConstraintUid) -> Option<&SpatialDimension<P>> {
        self.dimensions.iter().find(|d| d.id == id)
    }

    /// The first point, curve or text id not used yet (they share the
    /// numbers).
    pub fn next_entity(&self) -> EntityUid {
        let used = self
            .points
            .iter()
            .map(|p| p.id.0)
            .chain(self.curves.iter().map(|c| c.id.0))
            .chain(self.texts.iter().map(|t| t.planar.id.0))
            .chain(self.planar.derived_ids().map(|id| id.0));
        EntityUid(used.max().map_or(1, |n| n + 1))
    }

    /// The first constraint or dimension id not used yet (they share the
    /// numbers, with the kept patterns and offsets).
    pub fn next_constraint(&self) -> ConstraintUid {
        let used = self
            .constraints
            .iter()
            .map(|c| c.id.0)
            .chain(self.dimensions.iter().map(|d| d.id.0))
            .chain(self.planar.constraint_ids().map(|k| k.0));
        ConstraintUid(used.max().map_or(1, |n| n + 1))
    }

    /// The attachments: each relation's id, its point and its source.
    pub fn attachments(&self) -> impl Iterator<Item = (ConstraintUid, EntityUid, &SpatialSource)> {
        self.attachment_records()
            .map(|(id, point, source, _)| (id, point, source))
    }

    /// The attachments with whether each is projected onto the sketch's
    /// plane (Project) rather than included in true 3D.
    pub fn attachment_records(
        &self,
    ) -> impl Iterator<Item = (ConstraintUid, EntityUid, &SpatialSource, bool)> {
        self.constraints.iter().filter_map(|c| match &c.kind {
            SpatialConstraintKind::Attach {
                point,
                source,
                projected,
            } => Some((c.id, *point, source, *projected)),
            _ => None,
        })
    }

    /// The points on faces (mitcad#1549): each On Face relation's id, its
    /// point and its face.
    pub fn face_records(&self) -> impl Iterator<Item = (ConstraintUid, EntityUid, &SpatialSource)> {
        self.constraints.iter().filter_map(|c| match &c.kind {
            SpatialConstraintKind::OnFace { point, source } => Some((c.id, *point, source)),
            _ => None,
        })
    }

    /// The planes found outside the sketch that its Symmetric and On Plane
    /// relations take (mitcad#1661), by relation.
    pub fn plane_records(&self) -> impl Iterator<Item = (ConstraintUid, &SpatialPlaneRef)> {
        self.constraints
            .iter()
            .filter_map(|c| c.kind.plane().filter(|p| p.is_outside()).map(|p| (c.id, p)))
    }

    /// The reference curves: each one's id and source.
    pub fn reference_curves(&self) -> impl Iterator<Item = (EntityUid, &SpatialSource)> {
        self.reference_records().map(|(id, source, _)| (id, source))
    }

    /// The reference curves with whether each is projected onto the
    /// sketch's plane (Project) rather than included in true 3D.
    pub fn reference_records(&self) -> impl Iterator<Item = (EntityUid, &SpatialSource, bool)> {
        self.curves.iter().filter_map(|c| match &c.kind {
            SpatialCurveKind::Reference { source, projected } => Some((c.id, source, *projected)),
            _ => None,
        })
    }

    /// The reference curve of `source` made the same way (`projected`),
    /// when the sketch has one: one record per source element and
    /// occurrence, reused by every point put on it (mitcad#1543).
    pub fn reference_of(&self, source: &SpatialSource, projected: bool) -> Option<EntityUid> {
        self.reference_records()
            .find(|(_, s, p)| *s == source && *p == projected)
            .map(|(id, _, _)| id)
    }

    /// What uses a reference curve: the relations and dimensions that name
    /// it.
    pub fn reference_users(&self, curve: EntityUid) -> Vec<ConstraintUid> {
        self.relations_of(curve)
    }

    /// The curves that `point` defines (a line's ends, an arc's centre, a
    /// spline's points).
    pub fn curves_at(&self, point: EntityUid) -> Vec<EntityUid> {
        self.curves
            .iter()
            .filter(|c| c.points().contains(&point))
            .map(|c| c.id)
            .collect()
    }

    /// The texts whose frame uses `point`.
    pub fn texts_at(&self, point: EntityUid) -> Vec<EntityUid> {
        self.texts
            .iter()
            .filter(|t| t.planar.frame.contains(&point))
            .map(|t| t.planar.id)
            .collect()
    }

    /// The relations and dimensions that name `entity`: a point, or a
    /// curve.
    pub fn relations_of(&self, entity: EntityUid) -> Vec<ConstraintUid> {
        let names = |points: Vec<EntityUid>, curves: Vec<EntityUid>| {
            points.contains(&entity) || curves.contains(&entity)
        };
        self.constraints
            .iter()
            .filter(|c| names(c.kind.points(), c.kind.curves()))
            .map(|c| c.id)
            .chain(
                self.dimensions
                    .iter()
                    .filter(|d| names(d.kind.points(), d.kind.curves()))
                    .map(|d| d.id),
            )
            .collect()
    }

    /// The in-plane relations of the sketch plane as planar constraints
    /// (for measuring an in-plane spacing dimension).
    pub fn in_plane_constraints(&self) -> Vec<Constraint> {
        self.in_plane_constraints_of(SpatialAxis::Z)
    }

    /// The in-plane relations of the plane square to `normal` as planar
    /// constraints.
    pub fn in_plane_constraints_of(&self, normal: SpatialAxis) -> Vec<Constraint> {
        self.constraints
            .iter()
            .filter_map(|c| match &c.kind {
                SpatialConstraintKind::InPlane {
                    relation,
                    normal: n,
                } if *n == normal => Some(Constraint::new(c.id, relation.clone())),
                _ => None,
            })
            .collect()
    }

    /// Each point's elevation group, by its representative (the group's
    /// lowest id): points joined by coincidences, the points of each
    /// curve bound to a plane parallel to the sketch plane (an arc's or
    /// circle's centre, start and end, an ellipse's points) and the points
    /// a Plane-Bound relation keeps in such a plane (mitcad#1550), whose
    /// elevations are tied.
    pub fn elevation_groups(&self) -> BTreeMap<EntityUid, EntityUid> {
        let mut parent: BTreeMap<EntityUid, EntityUid> =
            self.points.iter().map(|p| (p.id, p.id)).collect();
        fn find(parent: &BTreeMap<EntityUid, EntityUid>, mut id: EntityUid) -> EntityUid {
            while let Some(&up) = parent.get(&id) {
                if up == id {
                    break;
                }
                id = up;
            }
            id
        }
        let mut join = |a: EntityUid, b: EntityUid| {
            let (ra, rb) = (find(&parent, a), find(&parent, b));
            if ra != rb && parent.contains_key(&ra) && parent.contains_key(&rb) {
                parent.insert(ra.max(rb), ra.min(rb));
            }
        };
        for c in &self.constraints {
            match &c.kind {
                SpatialConstraintKind::Coincident { a, b } => join(*a, *b),
                SpatialConstraintKind::PlaneBound {
                    points,
                    normal: SpatialAxis::Z,
                } => {
                    for p in points.iter().skip(1) {
                        join(points[0], *p);
                    }
                }
                _ => {}
            }
        }
        for c in self
            .curves
            .iter()
            .filter(|c| c.plane_normal() == Some(SpatialAxis::Z))
        {
            let points = c.points();
            for p in points.iter().skip(1) {
                join(points[0], *p);
            }
        }
        let ids: Vec<EntityUid> = parent.keys().copied().collect();
        ids.into_iter().map(|id| (id, find(&parent, id))).collect()
    }

    /// The points that keep `points` on the sketch plane with one
    /// [`SpatialConstraintKind::OnSketchPlane`] row each: the representative
    /// of each of their elevation groups, ascending. A planar sketch
    /// converted with them keeps its degrees of freedom and adds no
    /// redundancy (each new elevation is determined exactly once).
    pub fn elevation_representatives(&self, points: &[EntityUid]) -> Vec<EntityUid> {
        let groups = self.elevation_groups();
        let set: BTreeSet<EntityUid> = points
            .iter()
            .filter_map(|p| groups.get(p))
            .copied()
            .collect();
        set.into_iter().collect()
    }

    /// The sketch's On Sketch Plane relation, if it has one.
    pub fn on_sketch_plane(&self) -> Option<&SpatialConstraintDef> {
        self.constraints
            .iter()
            .find(|c| matches!(c.kind, SpatialConstraintKind::OnSketchPlane { .. }))
    }

    /// Checks the definition's own consistency: unique ids, curves through
    /// existing, distinct points with valid values, relations and
    /// dimensions of existing entities of the kinds they take, finite
    /// coordinates.
    pub fn check_definition(&self) -> Result<(), String> {
        if let Some(frame) = &self.frame {
            frame.check()?;
        }
        // In plain words for a person, the ids for agents (mitcad#1644).
        let twice = |named: String| crate::said("an id of the sketch is used twice", named);
        let mut entities = BTreeSet::new();
        for p in &self.points {
            if !entities.insert(p.id) {
                return Err(twice(format!("the id {} is used twice", p.id.0)));
            }
            if !finite3(p.at) {
                return Err(crate::said(
                    "a point's coordinate is not finite",
                    format!("p{}: a coordinate is not finite", p.id.0),
                ));
            }
        }
        let point = |id: EntityUid, what: &str| -> Result<(), String> {
            if self.point(id).is_some() {
                Ok(())
            } else {
                Err(crate::said(
                    "the sketch names a point that does not exist",
                    format!("{what}: point p{} does not exist", id.0),
                ))
            }
        };
        for c in &self.curves {
            if !entities.insert(c.id) {
                return Err(twice(format!("the id {} is used twice", c.id.0)));
            }
            let what = format!("{} c{}", c.type_name(), c.id.0);
            for p in c.points() {
                point(p, &what)?;
            }
            check_curve(c).map_err(|e| said_after("", &format!("{what}: "), &e))?;
            // A reference curve's source is a curve (mitcad#1543).
            if let SpatialCurveKind::Reference { source, .. } = &c.kind
                && !source.is_curve()
            {
                return Err(said_at(
                    "a reference curve's source is an edge, a sketch's curve or an Intersection \
                     Curve, not a point or a face",
                    format!(
                        "{what}: {source} is a {}; a reference curve's source is an edge, a \
                         sketch's curve or an Intersection Curve",
                        source.kind_name()
                    ),
                    source.places(),
                ));
            }
        }
        for t in &self.texts {
            if !entities.insert(t.planar.id) {
                return Err(twice(format!("the id {} is used twice", t.planar.id.0)));
            }
            if !t.z.is_finite() {
                return Err(crate::said(
                    "a text's elevation is not finite",
                    format!("text t{}: its elevation is not finite", t.planar.id.0),
                ));
            }
            for p in &t.planar.frame {
                point(*p, &format!("text t{}", t.planar.id.0))?;
            }
        }
        let curve = |id: EntityUid, what: &str| {
            self.curve(id).ok_or_else(|| {
                crate::said(
                    "the sketch names a curve that does not exist",
                    format!("{what}: curve c{} does not exist", id.0),
                )
            })
        };
        // `said`: what the relation or dimension takes, in plain words.
        let wrong = |said: &str, what: &str, c: &SpatialCurve, wanted: &str| {
            Err(crate::said(
                said,
                format!("{what}: c{} is a {}, not {wanted}", c.id.0, c.type_name()),
            ))
        };
        let mut relations = BTreeSet::new();
        for c in &self.constraints {
            let what = format!("{} ({})", c.id, c.kind.type_name());
            if !relations.insert(c.id) {
                return Err(twice(format!("the id {} is used twice", c.id)));
            }
            for p in c.kind.points() {
                point(p, &what)?;
            }
            for id in c.kind.curves() {
                curve(id, &what)?;
            }
            match &c.kind {
                SpatialConstraintKind::Coincident { a, b } if a == b => {
                    return Err(crate::said(
                        "a point cannot coincide with itself",
                        format!("{what}: a point cannot coincide with itself"),
                    ));
                }
                SpatialConstraintKind::Fix { at, .. } if !finite3(*at) => {
                    return Err(crate::said(
                        "a fixed point's place is not finite",
                        format!("{what}: a coordinate is not finite"),
                    ));
                }
                // An attachment's source is a point (mitcad#1543): a point
                // lies on a curve through its reference curve.
                SpatialConstraintKind::Attach { source, .. } if source.is_curve() => {
                    return Err(said_at(
                        "an attachment's source is a curve: a point is attached to a point (a \
                         curve's start, end or middle), or put on the curve's reference curve",
                        format!(
                            "{what}: {source} is a curve; a point is attached to a point (a \
                             curve's start, end or middle), or put on the curve's reference curve"
                        ),
                        source.places(),
                    ));
                }
                // A face's record is an On Face relation (mitcad#1549).
                SpatialConstraintKind::Attach { source, .. } if source.is_face() => {
                    return Err(said_at(
                        "an attachment's source is a face: a point lies on a face through an On \
                         Face relation",
                        format!(
                            "{what}: {source} is a face; a point lies on a face through an On \
                             Face relation"
                        ),
                        source.places(),
                    ));
                }
                SpatialConstraintKind::OnFace { source, .. } if !source.is_face() => {
                    return Err(said_at(
                        "an On Face relation's source is a face of a body, not a point or a curve",
                        format!(
                            "{what}: {source} is a {}; an On Face relation's source is a face of \
                             a body",
                            source.kind_name()
                        ),
                        source.places(),
                    ));
                }
                SpatialConstraintKind::Align { line, .. } => {
                    let l = curve(*line, &what)?;
                    if l.as_line().is_none() {
                        return wrong("an alignment takes a line", &what, l, "a line");
                    }
                }
                SpatialConstraintKind::NormalAlign { curve: id, .. } => {
                    let r = curve(*id, &what)?;
                    if !r.is_round() || r.is_plane_bound() {
                        return wrong(
                            "a normal alignment takes an arc or circle with a free normal",
                            &what,
                            r,
                            "an arc or circle with a free normal",
                        );
                    }
                }
                SpatialConstraintKind::PointOn {
                    point: p,
                    curve: id,
                }
                | SpatialConstraintKind::Midpoint {
                    point: p,
                    curve: id,
                } => {
                    if curve(*id, &what)?.points().contains(p) {
                        return Err(crate::said(
                            "a point cannot lie on a curve it defines",
                            format!("{what}: p{} defines c{}", p.0, id.0),
                        ));
                    }
                }
                SpatialConstraintKind::Tangent { a, b, .. } if a == b => {
                    return Err(crate::said(
                        "a tangency needs two curves",
                        format!("{what}: a tangency needs two curves"),
                    ));
                }
                SpatialConstraintKind::Equal { a, b } => {
                    let (ca, cb) = (curve(*a, &what)?, curve(*b, &what)?);
                    let lines = ca.as_line().is_some() && cb.as_line().is_some();
                    if a == b || !(lines || (ca.is_round() && cb.is_round())) {
                        return Err(crate::said(
                            "equal takes two lines or two arcs or circles",
                            format!(
                                "{what}: equal lengths of two lines or radii of two arcs or \
                                 circles"
                            ),
                        ));
                    }
                }
                SpatialConstraintKind::OnSketchPlane { points } => {
                    let unique: BTreeSet<&EntityUid> = points.iter().collect();
                    if points.is_empty() || unique.len() != points.len() {
                        return Err(crate::said(
                            "On Sketch Plane lists each point once, at least one",
                            format!("{what}: it lists each point once, at least one"),
                        ));
                    }
                }
                SpatialConstraintKind::PlaneBound { points, .. } => {
                    let unique: BTreeSet<&EntityUid> = points.iter().collect();
                    if points.len() < 2 || unique.len() != points.len() {
                        return Err(crate::said(
                            "a shape's plane lists each of its points once, at least two",
                            format!("{what}: it lists each point once, at least two"),
                        ));
                    }
                }
                // A rotated plane's relations (mitcad#1661): their frame or
                // direction unit and square.
                SpatialConstraintKind::InFrame { frame, .. }
                | SpatialConstraintKind::FrameBound { frame, .. }
                    if !frame.is_valid() =>
                {
                    return Err(crate::said(
                        "a rotated plane's axes are not unit and square",
                        format!("{what}: the frame's x and normal are not unit and square"),
                    ));
                }
                SpatialConstraintKind::AlignAlong { direction, .. }
                | SpatialConstraintKind::NormalAlong { direction, .. }
                    if !finite3(*direction) || (norm(*direction) - 1.0).abs() > 1e-9 =>
                {
                    return Err(crate::said(
                        "a direction is not of unit length",
                        format!("{what}: the direction is not of unit length"),
                    ));
                }
                SpatialConstraintKind::FrameBound { points, .. } => {
                    let unique: BTreeSet<&EntityUid> = points.iter().collect();
                    if points.len() < 2 || unique.len() != points.len() {
                        return Err(crate::said(
                            "a shape's plane lists each of its points once, at least two",
                            format!("{what}: it lists each point once, at least two"),
                        ));
                    }
                }
                SpatialConstraintKind::AlignAlong { line, .. } => {
                    let l = curve(*line, &what)?;
                    if l.as_line().is_none() {
                        return Err(crate::said(
                            "an alignment along a direction takes a line",
                            format!("{what}: c{} is not a line", line.0),
                        ));
                    }
                }
                SpatialConstraintKind::NormalAlong { curve: id, .. } => {
                    let r = curve(*id, &what)?;
                    if !r.is_round() || r.is_plane_bound() {
                        return Err(crate::said(
                            "a normal along a direction takes an arc or circle with a free normal",
                            format!("{what}: c{} is no arc or circle with a free normal", id.0),
                        ));
                    }
                }
                SpatialConstraintKind::Parallel { a, b } => {
                    self.check_directions(*a, *b, &what, "parallel")?;
                }
                SpatialConstraintKind::Perpendicular { a, b } => {
                    self.check_directions(*a, *b, &what, "perpendicular")?;
                }
                SpatialConstraintKind::Symmetric { a, b, plane } => {
                    if a == b {
                        return Err(crate::said(
                            "a symmetry needs two points",
                            format!("{what}: a symmetry needs two points"),
                        ));
                    }
                    check_plane(plane, &what)?;
                }
                SpatialConstraintKind::OnPlane { points, plane } => {
                    let unique: BTreeSet<&EntityUid> = points.iter().collect();
                    if points.is_empty() || unique.len() != points.len() {
                        return Err(crate::said(
                            "On Plane lists each point once, at least one",
                            format!("{what}: it lists each point once, at least one"),
                        ));
                    }
                    check_plane(plane, &what)?;
                }
                SpatialConstraintKind::Concentric { a, b, .. } => {
                    let (ca, cb) = (curve(*a, &what)?, curve(*b, &what)?);
                    if a == b || !ca.is_round() || !cb.is_round() {
                        return Err(crate::said(
                            "concentric takes two arcs or circles",
                            format!("{what}: concentric takes two arcs or circles"),
                        ));
                    }
                }
                SpatialConstraintKind::Curvature { a, b, .. } => {
                    let (ca, cb) = (curve(*a, &what)?, curve(*b, &what)?);
                    let joins =
                        |c: &SpatialCurve| c.is_spline() || c.is_round() || c.as_line().is_some();
                    if a == b || !(ca.is_spline() || cb.is_spline()) || !joins(ca) || !joins(cb) {
                        return Err(crate::said(
                            "curvature continuity joins a spline's end to a line, an arc or \
                             another spline",
                            format!(
                                "{what}: curvature continuity joins a spline's end to a line, an \
                                 arc or another spline"
                            ),
                        ));
                    }
                }
                _ => {}
            }
        }
        for d in &self.dimensions {
            let what = format!("{} ({})", d.id, d.kind.type_name());
            if !relations.insert(d.id) {
                return Err(twice(format!("the id {} is used twice", d.id)));
            }
            for p in d.kind.points() {
                point(p, &what)?;
            }
            for id in d.kind.curves() {
                curve(id, &what)?;
            }
            match &d.kind {
                SpatialDimensionKind::Length { line } => {
                    let l = curve(*line, &what)?;
                    if l.as_line().is_none() {
                        return wrong("a length dimension takes a line", &what, l, "a line");
                    }
                }
                SpatialDimensionKind::Offset { a, b, .. } if a == b => {
                    return Err(crate::said(
                        "an offset needs two points",
                        format!("{what}: an offset needs two points"),
                    ));
                }
                SpatialDimensionKind::Radius { curve: id }
                | SpatialDimensionKind::Diameter { curve: id } => {
                    let r = curve(*id, &what)?;
                    if !r.is_round() {
                        return wrong(
                            "a radius or diameter takes an arc or circle",
                            &what,
                            r,
                            "an arc or circle",
                        );
                    }
                }
                SpatialDimensionKind::ArcLength { curve: id } => {
                    let r = curve(*id, &what)?;
                    if !matches!(r.kind, SpatialCurveKind::Arc { .. }) {
                        return wrong("an arc length takes an arc", &what, r, "an arc");
                    }
                }
                SpatialDimensionKind::MajorRadius { curve: id }
                | SpatialDimensionKind::MinorRadius { curve: id } => {
                    let r = curve(*id, &what)?;
                    if !matches!(
                        r.kind,
                        SpatialCurveKind::Ellipse { .. } | SpatialCurveKind::EllipticalArc { .. }
                    ) {
                        return wrong(
                            "a major or minor radius takes an ellipse",
                            &what,
                            r,
                            "an ellipse",
                        );
                    }
                }
                SpatialDimensionKind::SplineLength { curve: id } => {
                    let r = curve(*id, &what)?;
                    if !r.is_spline() {
                        return wrong("a spline length takes a spline", &what, r, "a spline");
                    }
                }
                SpatialDimensionKind::Angle { a, b } => {
                    self.check_directions(*a, *b, &what, "an angle")?;
                }
                SpatialDimensionKind::OffsetAlong { a, b, direction } => {
                    if a == b {
                        return Err(crate::said(
                            "an offset needs two points",
                            format!("{what}: an offset needs two points"),
                        ));
                    }
                    if !finite3(*direction) || (norm(*direction) - 1.0).abs() > 1e-9 {
                        return Err(crate::said(
                            "a direction is not of unit length",
                            format!("{what}: the direction is not of unit length"),
                        ));
                    }
                }
                SpatialDimensionKind::TangentMagnitude { curve: id, end } => {
                    self.check_tangent_end(*id, *end, &what)?;
                }
                _ => {}
            }
        }
        // The kept planar records (mitcad#1550): their ids are the
        // sketch's too.
        for e in &self.planar.derived {
            if !entities.insert(e.id) {
                return Err(twice(format!("the id {} is used twice", e.id.0)));
            }
        }
        for id in self.planar.constraint_ids() {
            if !relations.insert(id) {
                return Err(twice(format!("the id {id} is used twice")));
            }
        }
        for projection in &self.planar.projections {
            for r in &projection.entities {
                if !entities.contains(&r.uid()) {
                    return Err(crate::said(
                        "a projection lists an entity the sketch does not have",
                        format!(
                            "the projection of {} lists {r}, which the sketch does not have",
                            projection.source
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Checks the two directions of a parallel, perpendicular or angle
    /// relation (mitcad#1661): each a line or an arc or circle (its
    /// normal), two different ones, not both normals bound to frame axes.
    fn check_directions(
        &self,
        a: EntityUid,
        b: EntityUid,
        what: &str,
        relation: &str,
    ) -> Result<(), String> {
        let direction = |id: EntityUid| {
            self.curve(id)
                .filter(|c| c.as_line().is_some() || c.is_round())
        };
        let (Some(ca), Some(cb)) = (direction(a), direction(b)) else {
            return Err(crate::said(
                format!("{relation} takes two lines, or a line and an arc or circle (its normal)"),
                format!(
                    "{what}: {relation} takes two lines, or a line and an arc or circle (its \
                     normal)"
                ),
            ));
        };
        if a == b {
            return Err(crate::said(
                format!("{relation} needs two different curves"),
                format!("{what}: {relation} needs two different curves"),
            ));
        }
        if ca.is_plane_bound() && cb.is_plane_bound() {
            return Err(crate::said(
                format!("{relation} cannot change two normals bound to the sketch's planes"),
                format!("{what}: both normals are bound to frame axes"),
            ));
        }
        Ok(())
    }

    /// Whether a tangency or curvature continuity holds `curve` at its point `at` (or at a point
    /// coincident with it).
    fn tangent_at(&self, curve: EntityUid, at: EntityUid) -> bool {
        let joined = |p: EntityUid| {
            p == at
                || self.constraints.iter().any(|c| {
                    matches!(c.kind, SpatialConstraintKind::Coincident { a, b }
                        if (a == p && b == at) || (a == at && b == p))
                })
        };
        self.constraints.iter().any(|c| match c.kind {
            SpatialConstraintKind::Tangent {
                a, b, at: point, ..
            }
            | SpatialConstraintKind::Curvature {
                a, b, at: point, ..
            } => (a == curve || b == curve) && joined(point),
            _ => false,
        })
    }

    /// Checks a tangent magnitude's end (mitcad#1661): an open spline's
    /// start or end; a fit-point spline's end must be free or held by a
    /// tangency or curvature continuity, as a natural end has its tangent
    /// already.
    fn check_tangent_end(
        &self,
        id: EntityUid,
        end: SpatialCurveEnd,
        what: &str,
    ) -> Result<(), String> {
        let not_open = || {
            Err(crate::said(
                "a tangent magnitude takes the start or end of an open spline",
                format!("{what}: c{} is no open spline", id.0),
            ))
        };
        let Some(curve) = self.curve(id) else {
            return not_open();
        };
        match &curve.kind {
            SpatialCurveKind::Spline { .. } => Ok(()),
            SpatialCurveKind::FittedSpline {
                points,
                closed: false,
                ends,
                ..
            } if !points.is_empty() => {
                let at = if end == SpatialCurveEnd::Start {
                    points[0]
                } else {
                    points[points.len() - 1]
                };
                if ends[end.index()] == SpatialSplineEnd::Free || self.tangent_at(id, at) {
                    Ok(())
                } else {
                    Err(crate::said(
                        "a tangent magnitude needs a spline end that is free or tangent: a \
                         natural end has its tangent already",
                        format!(
                            "{what}: the {} of c{} is natural; make it free or tangent first",
                            if end == SpatialCurveEnd::Start {
                                "start"
                            } else {
                                "end"
                            },
                            id.0
                        ),
                    ))
                }
            }
            _ => not_open(),
        }
    }

    /// Where a stored point is.
    pub fn position(&self, id: EntityUid) -> Option<[f64; 3]> {
        self.point(id).map(|p| p.at)
    }

    /// An arc's or circle's stored unit normal (its plane's axis for a
    /// plane-bound one) and radius (an arc's from its stored points).
    pub fn stored_round(&self, id: EntityUid) -> Option<([f64; 3], f64)> {
        match &self.curve(id)?.kind {
            SpatialCurveKind::Arc {
                center,
                start,
                normal,
                plane,
                ..
            } => {
                let n = plane.map_or_else(|| unit(*normal), |p| Some(p.unit()))?;
                let r = norm(sub(self.position(*start)?, self.position(*center)?));
                Some((n, r))
            }
            SpatialCurveKind::Circle {
                radius,
                normal,
                plane,
                ..
            } => {
                let n = plane.map_or_else(|| unit(*normal), |p| Some(p.unit()))?;
                Some((n, *radius))
            }
            _ => None,
        }
    }

    /// What a dimension measures at the stored positions (None for a
    /// spline's length, which needs a solve).
    pub fn measure(&self, kind: &SpatialDimensionKind) -> Option<f64> {
        let at = |id: EntityUid| self.position(id);
        let round = |id: EntityUid| self.stored_round(id);
        let minor = |id: EntityUid| self.stored_minor(id);
        let normal = match kind {
            SpatialDimensionKind::InPlane { normal, .. } => *normal,
            _ => SpatialAxis::Z,
        };
        Geometry {
            curves: &self.curves,
            at: &at,
            round: &round,
            minor: &minor,
        }
        .measure(kind, &self.in_plane_constraints_of(normal))
    }

    /// An ellipse's or elliptical arc's stored minor radius.
    pub fn stored_minor(&self, id: EntityUid) -> Option<f64> {
        match self.curve(id)?.kind {
            SpatialCurveKind::Ellipse { minor_radius, .. }
            | SpatialCurveKind::EllipticalArc { minor_radius, .. } => Some(minor_radius),
            _ => None,
        }
    }

    /// The texts' outlines (mitcad#1550), laid out as a planar sketch lays
    /// its texts out (the kernel's fonts, or the outlines stored with a
    /// text), each in its own plane at its elevation, with the points of a
    /// text's frame where `at` puts them: every contour as a polyline of
    /// local points. Without font support (a test kernel) none.
    pub fn text_outlines<K: Kernel + ?Sized>(
        &self,
        at: &dyn Fn(EntityUid) -> Option<[f64; 3]>,
        kernel: &K,
    ) -> Result<Vec<TextOutline>, String> {
        let mut out = Vec::new();
        for t in &self.texts {
            // Its plane: a frame plane, or a rotated one (mitcad#1661).
            let plane = t.plane();
            let mut planar: SketchDef = SketchDef::default();
            planar.texts = vec![t.planar.clone()];
            let mut solved = Solved::default();
            for p in &t.planar.frame {
                if let Some(local) = at(*p) {
                    solved.points.insert(*p, plane.planar(local));
                }
            }
            // A text in a frame lies where its frame's corner is.
            let elevation = t
                .planar
                .frame
                .first()
                .and_then(|p| at(*p))
                .map_or(t.z, |corner| plane.elevation(corner));
            for outline in planar.text_outlines(&solved, kernel)? {
                let contours = outline
                    .contours
                    .iter()
                    .map(|c| {
                        c.parts
                            .iter()
                            .flat_map(|part| {
                                let (t0, t1) = part.domain();
                                part.sample(t0, t1)
                            })
                            .map(|uv| plane.place(uv, elevation))
                            .collect()
                    })
                    .collect();
                out.push((outline.id, contours));
            }
        }
        Ok(out)
    }

    /// The tangent at an end `point` of `curve` pointing away from the
    /// curve (the way a route continues there), from the stored geometry:
    /// a line's direction, an arc's tangent about its normal, a spline's
    /// first or last control leg (a fit-point spline's chord to its next
    /// fit point, close to its tangent).
    pub fn outward_tangent(&self, curve: EntityUid, point: EntityUid) -> Option<[f64; 3]> {
        let c = self.curve(curve)?;
        let [first, last] = c.ends()?;
        if point != first && point != last {
            return None;
        }
        let at = |id| self.position(id);
        let away = |from: EntityUid, to: EntityUid| unit(sub(at(to)?, at(from)?));
        match &c.kind {
            SpatialCurveKind::Line { start, end } => {
                if point == *end {
                    away(*start, *end)
                } else {
                    away(*end, *start)
                }
            }
            SpatialCurveKind::Arc { center, .. } => {
                let (n, _) = self.stored_round(curve)?;
                let along = unit(cross(n, sub(at(point)?, at(*center)?)))?;
                // Counter-clockwise from the start: forward at the end,
                // back at the start.
                Some(if point == last {
                    along
                } else {
                    along.map(|v| -v)
                })
            }
            SpatialCurveKind::Spline { control: p, .. }
            | SpatialCurveKind::FittedSpline { points: p, .. } => {
                if p.len() < 2 {
                    return None;
                }
                if point == last {
                    away(p[p.len() - 2], p[p.len() - 1])
                } else {
                    away(p[1], p[0])
                }
            }
            _ => None,
        }
    }
}

/// A curve's own values: distinct points, a unit normal, a radius, a
/// spline's degree, weights, knots and points.
pub(crate) fn check_curve(c: &SpatialCurve) -> Result<(), String> {
    let positive = |v: f64| v.is_finite() && v > SPATIAL_MIN_SEGMENT;
    let distinct = |points: &[EntityUid]| {
        let set: BTreeSet<&EntityUid> = points.iter().collect();
        set.len() == points.len()
    };
    let normal_ok = |n: &[f64; 3], plane: &Option<SpatialDirection>| {
        plane.is_some() || (finite3(*n) && unit(*n).is_some())
    };
    match &c.kind {
        SpatialCurveKind::Line { start, end } if start == end => {
            Err("a line starts and ends at one point: it needs two points".to_owned())
        }
        SpatialCurveKind::Arc { normal, plane, .. } => {
            if !distinct(&c.points()) {
                Err("an arc needs three distinct points".to_owned())
            } else if !normal_ok(normal, plane) {
                Err("an arc's normal is not a finite direction".to_owned())
            } else {
                Ok(())
            }
        }
        SpatialCurveKind::Circle {
            radius,
            normal,
            plane,
            ..
        } => {
            if !positive(*radius) {
                Err(format!(
                    "a circle's radius must be longer than {SPATIAL_MIN_SEGMENT} mm"
                ))
            } else if !normal_ok(normal, plane) {
                Err("a circle's normal is not a finite direction".to_owned())
            } else {
                Ok(())
            }
        }
        SpatialCurveKind::Spline {
            degree,
            control,
            weights,
            knots,
        } => {
            let (n, p) = (control.len(), *degree as usize);
            if p == 0 || n < p + 1 {
                Err("a spline needs degree 1 or more and degree + 1 control points".to_owned())
            } else if !weights.is_empty()
                && (weights.len() != n || !weights.iter().all(|w| w.is_finite() && *w > 0.0))
            {
                Err("a spline needs one positive weight per control point".to_owned())
            } else if !knots.is_empty()
                && (knots.len() != n + p + 1
                    || !knots.iter().all(|k| k.is_finite())
                    || knots.windows(2).any(|w| w[1] < w[0]))
            {
                Err(
                    "a spline's knots are control points + degree + 1 non-decreasing values"
                        .to_owned(),
                )
            } else if control.windows(2).any(|w| w[0] == w[1]) {
                Err("a spline's successive control points must be distinct".to_owned())
            } else {
                Ok(())
            }
        }
        SpatialCurveKind::FittedSpline { points, closed, .. } => {
            let least = if *closed { 3 } else { 2 };
            if points.len() < least {
                Err(format!("a fit-point spline needs at least {least} points"))
            } else if !distinct(points) {
                Err("a fit-point spline lists each fit point once".to_owned())
            } else {
                Ok(())
            }
        }
        SpatialCurveKind::Ellipse { minor_radius, .. }
        | SpatialCurveKind::EllipticalArc { minor_radius, .. } => {
            if !distinct(&c.points()) {
                Err("an ellipse's points must be distinct".to_owned())
            } else if !positive(*minor_radius) {
                Err(format!(
                    "an ellipse's minor radius must be longer than {SPATIAL_MIN_SEGMENT} mm"
                ))
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

/// What a sketch is built with besides its definition and parameters:
/// every attachment's local target (the origin's is `[0, 0, 0]` when it is
/// not given), every reference curve's geometry in local coordinates
/// ([`local_reference_curve`]) and every On Face relation's surface in
/// local coordinates ([`local_surface`]; for a face that is no plane,
/// cylinder, cone, sphere or torus, the plane touching it nearest to the
/// point), and every Symmetric or On Plane relation's plane found outside
/// the sketch (mitcad#1661: a construction plane, a planar face, another
/// sketch's plane), found before the solve.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialInputs {
    pub targets: BTreeMap<ConstraintUid, [f64; 3]>,
    pub curves: BTreeMap<EntityUid, SpatialReferenceCurve>,
    pub surfaces: BTreeMap<ConstraintUid, SpatialSurface>,
}

impl SpatialSketchDef {
    /// Rejects values a driving dimension cannot have: one that is not
    /// finite, a length, radius, diameter or arc length not greater than
    /// [`SPATIAL_MIN_SEGMENT`], an in-plane value outside its planar range.
    pub fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        for d in &self.dimensions {
            let Some(param) = d.value else {
                continue;
            };
            check_value(d.id, &d.kind, value(param))?;
        }
        Ok(())
    }

    /// What a spline's length or end tangent magnitude measures where the
    /// sketch's points are stored (mitcad#1661: a driving one made at its
    /// measured value): the spline alone, solved from its stored points
    /// and end tangents as the next solve starts. None for another kind.
    pub fn measure_spline(&self, kind: &SpatialDimensionKind) -> Option<f64> {
        let (id, end) = match kind {
            SpatialDimensionKind::SplineLength { curve } => (*curve, None),
            SpatialDimensionKind::TangentMagnitude { curve, end } => (*curve, Some(end.index())),
            _ => return None,
        };
        let curve = self.curve(id)?.clone();
        let points = curve
            .points()
            .iter()
            .filter_map(|p| self.point(*p).cloned())
            .collect();
        let alone = SpatialSketchDef {
            support: self.support.clone(),
            points,
            curves: vec![curve],
            ..SpatialSketchDef::default()
        };
        let mut no_values = |_: ParamId| -> Result<f64, String> { Err("no values".to_owned()) };
        let mut model = alone.build(&mut no_values, &BTreeMap::new()).ok()?;
        model.solve().ok()?;
        let Some(SolverCurve::Curve(c)) = model.curve_of.get(&id).copied() else {
            return None;
        };
        match end {
            None => model.system.spline_length(c),
            Some(end) => model.system.tangent_length(c, end),
        }
    }

    /// The solver's system of the definition with the attachments' local
    /// targets only (no reference curves): [`SpatialSketchDef::build_with`].
    pub fn build(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        targets: &BTreeMap<ConstraintUid, [f64; 3]>,
    ) -> Result<SpatialModel, SpatialFailure> {
        self.build_with(
            value,
            &SpatialInputs {
                targets: targets.clone(),
                ..SpatialInputs::default()
            },
        )
    }

    /// The solver's system of the definition: the points where they are
    /// stored, arcs and circles with their stored normals and radii,
    /// `value` giving the driving dimensions' values and `inputs` the
    /// attachments' targets and the reference curves. Driven dimensions add
    /// nothing; a tangency keeps its stored sense.
    pub fn build_with(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
        inputs: &SpatialInputs,
    ) -> Result<SpatialModel, SpatialFailure> {
        self.check_definition()
            .map_err(|e| SpatialFailure::new(SpatialFailureKind::Definition, e))?;
        let mut system = SpatialSystem::new();
        let mut point_of = BTreeMap::new();
        let mut point_owner = BTreeMap::new();
        for p in &self.points {
            let id = system.add_point(p.at).map_err(|e| SpatialFailure {
                kind: SpatialFailureKind::Definition,
                message: said_after("", &format!("p{}: ", p.id.0), &e.to_string()),
                constraints: Vec::new(),
                entities: vec![p.id],
            })?;
            point_of.insert(p.id, id);
            point_owner.insert(id.index(), p.id);
        }
        let mut model = SpatialModel {
            system,
            point_owner,
            point_of,
            curve_of: BTreeMap::new(),
            line_owner: BTreeMap::new(),
            curve_owner: BTreeMap::new(),
            rows: BTreeMap::new(),
            tangents: Vec::new(),
            faces: BTreeMap::new(),
            curves: self.curves.clone(),
            in_plane: [SpatialAxis::X, SpatialAxis::Y, SpatialAxis::Z]
                .into_iter()
                .map(|axis| (axis, self.in_plane_constraints_of(axis)))
                .collect(),
            dimensions: self
                .dimensions
                .iter()
                .map(|d| (d.id, d.kind.clone()))
                .collect(),
            frames: BTreeMap::new(),
        };
        for c in &self.curves {
            model.add_curve(c, inputs)?;
        }
        // The kept planar records (mitcad#1550): inactive relations hold
        // nothing; patterns and offsets bind in the plane, and what their
        // corners imply is left out, as in a planar sketch.
        let inactive = self.planar.inactive_ids();
        let (bindings, left_out) = self
            .in_plane_bindings(&mut *value)
            .map_err(|e| SpatialFailure::new(SpatialFailureKind::Value, e))?;
        let applied = |id: ConstraintUid| !inactive.contains(&id) && !left_out.contains(&id);
        // The planar stand-ins of each frame plane's in-plane relations
        // (mitcad#1550); the kept planar records bind in the sketch plane.
        let mut plane: BTreeMap<SpatialAxis, InPlaneIds> = BTreeMap::new();
        for c in self.constraints.iter().filter(|c| applied(c.id)) {
            model.add_relation(c, inputs, &mut plane)?;
        }
        for b in &bindings {
            let sketch_plane = plane
                .entry(SpatialAxis::Z)
                .or_insert_with_key(|axis| InPlaneIds::new(*axis));
            model.add_binding(b, sketch_plane)?;
        }
        for d in self.dimensions.iter().filter(|d| applied(d.id)) {
            let Some(param) = d.value else {
                continue;
            };
            let value_failure = |message: String| SpatialFailure {
                kind: SpatialFailureKind::Value,
                message,
                constraints: vec![d.id],
                entities: Vec::new(),
            };
            let v = value(param)
                .map_err(|e| value_failure(said_after("", &format!("{}: ", d.id), &e)))?;
            check_value(d.id, &d.kind, v).map_err(value_failure)?;
            model.add_dimension(d.id, &d.kind, v, &mut plane)?;
        }
        Ok(model)
    }

    /// Writes a solve into the definition as the next solve's start: the
    /// points' positions, the free normals of arcs and circles, circles'
    /// radii and the senses of tangencies made without one.
    pub fn store_solved(&mut self, solved: &SpatialSolved) {
        for point in &mut self.points {
            if let Some(at) = solved.points.get(&point.id) {
                point.at = *at;
            }
        }
        for curve in &mut self.curves {
            // An ellipse's solved minor radius (mitcad#1550).
            if let (
                Some(SolvedCurve::Ellipse { minor, .. }),
                SpatialCurveKind::Ellipse { minor_radius, .. }
                | SpatialCurveKind::EllipticalArc { minor_radius, .. },
            ) = (solved.curves.get(&curve.id), &mut curve.kind)
            {
                *minor_radius = *minor;
                continue;
            }
            let Some(SolvedCurve::Round { normal, radius }) = solved.curves.get(&curve.id) else {
                continue;
            };
            match &mut curve.kind {
                SpatialCurveKind::Arc {
                    normal: n,
                    plane: None,
                    ..
                } => *n = *normal,
                SpatialCurveKind::Circle {
                    normal: n,
                    radius: r,
                    plane,
                    ..
                } => {
                    if plane.is_none() {
                        *n = *normal;
                    }
                    *r = *radius;
                }
                _ => {}
            }
        }
        for c in &mut self.constraints {
            if let SpatialConstraintKind::Tangent { sense, .. }
            | SpatialConstraintKind::Curvature { sense, .. } = &mut c.kind
                && let Some(solved) = solved.tangent_senses.get(&c.id)
            {
                *sense = Some(*solved);
            }
        }
        // The end tangents of free and tangent ends (mitcad#1526).
        let held: Vec<[bool; 2]> = self
            .curves
            .iter()
            .map(|curve| match &curve.kind {
                SpatialCurveKind::FittedSpline {
                    points,
                    closed: false,
                    ends,
                    ..
                } if !points.is_empty() => [0, 1].map(|end| {
                    let at = if end == 0 {
                        points[0]
                    } else {
                        points[points.len() - 1]
                    };
                    ends[end] == SpatialSplineEnd::Free || self.tangent_at(curve.id, at)
                }),
                _ => [false; 2],
            })
            .collect();
        for (curve, held) in self.curves.iter_mut().zip(held) {
            let SpatialCurveKind::FittedSpline {
                closed: false,
                tangents,
                ..
            } = &mut curve.kind
            else {
                continue;
            };
            let geometry = match solved.curves.get(&curve.id) {
                Some(SolvedCurve::Spline(g)) => Some(g),
                _ => None,
            };
            for (end, (slot, held)) in tangents.iter_mut().zip(held).enumerate() {
                *slot = if held {
                    geometry.and_then(|g| spline_end_tangent(g, end))
                } else {
                    None
                };
            }
        }
    }

    /// Whether `curve` has a tangent handle at `end` (0 its first point, 1
    /// its last) that a drag may pull (mitcad#1526): an open fit-point
    /// spline's end that is free or held by a tangency; refused naming why
    /// otherwise.
    pub fn tangent_handle(&self, curve: EntityUid, end: usize) -> Result<(), String> {
        let name = format!("c{}", curve.0);
        let Some(c) = self.curve(curve) else {
            return Err(crate::said(
                "the curve does not exist",
                format!("curve {name} does not exist"),
            ));
        };
        let SpatialCurveKind::FittedSpline {
            points,
            closed: false,
            ends,
            ..
        } = &c.kind
        else {
            return Err(crate::said(
                "only an open fit-point spline has tangent handles",
                format!("{name} is no open fit-point spline: only those have tangent handles"),
            ));
        };
        if end > 1 || points.is_empty() {
            return Err("a tangent handle is at the start (0) or the end (1)".to_owned());
        }
        let at = if end == 0 {
            points[0]
        } else {
            points[points.len() - 1]
        };
        if ends[end] == SpatialSplineEnd::Free || self.tangent_at(curve, at) {
            Ok(())
        } else {
            // One whole message per end (mitcad#1644).
            let said = if end == 0 {
                "the spline's start is natural: make it free or tangent to drag its tangent"
            } else {
                "the spline's end is natural: make it free or tangent to drag its tangent"
            };
            Err(crate::said(
                said,
                format!(
                    "{name}'s {} is natural: make it free or tangent to drag its tangent",
                    if end == 0 { "start" } else { "end" }
                ),
            ))
        }
    }
}

/// Checks a Symmetric or On Plane relation's plane (mitcad#1661): a face
/// source is a face.
fn check_plane(plane: &SpatialPlaneRef, what: &str) -> Result<(), String> {
    match plane {
        SpatialPlaneRef::Face(source) if !source.is_face() => Err(said_at(
            "the plane of a relation is a frame plane, a construction plane, a planar face or a \
             sketch's plane",
            format!(
                "{what}: {source} is a {}; a relation's plane is a frame plane, a construction \
                 plane, a planar face or a sketch's plane",
                source.kind_name()
            ),
            source.places(),
        )),
        _ => Ok(()),
    }
}

fn check_value(id: ConstraintUid, kind: &SpatialDimensionKind, value: f64) -> Result<(), String> {
    // In plain words for a person, the dimension's id for agents
    // (mitcad#1644).
    if !value.is_finite() {
        return Err(crate::said(
            "a dimension's value is not finite",
            format!("{id}: the value is not finite"),
        ));
    }
    let positive = match kind {
        SpatialDimensionKind::Length { .. }
        | SpatialDimensionKind::Radius { .. }
        | SpatialDimensionKind::Diameter { .. }
        | SpatialDimensionKind::MajorRadius { .. }
        | SpatialDimensionKind::MinorRadius { .. }
        | SpatialDimensionKind::ArcLength { .. } => true,
        SpatialDimensionKind::InPlane { dimension, .. } => match dimension.range() {
            ValueRange::Positive => true,
            ValueRange::NotNegative if value < 0.0 => {
                return Err(crate::said(
                    format!("a dimension's value cannot be negative, got {value}"),
                    format!("{id}: the value cannot be negative, got {value}"),
                ));
            }
            _ => false,
        },
        SpatialDimensionKind::SplineLength { .. }
        | SpatialDimensionKind::TangentMagnitude { .. } => true,
        SpatialDimensionKind::Angle { .. } => {
            if value <= 0.0 || value >= std::f64::consts::PI {
                return Err(crate::said(
                    "an angle between two directions must be greater than 0° and less than 180°; \
                     make them parallel instead",
                    format!(
                        "{id}: an angle must be greater than 0 and less than pi radians, got \
                         {value}"
                    ),
                ));
            }
            false
        }
        SpatialDimensionKind::Offset { .. } | SpatialDimensionKind::OffsetAlong { .. } => false,
    };
    if positive && value <= SPATIAL_MIN_SEGMENT {
        let name = match kind {
            SpatialDimensionKind::InPlane { dimension, .. } => dimension.type_name(),
            _ => kind.type_name(),
        };
        return Err(crate::said(
            format!(
                "a dimension's value must be greater than {SPATIAL_MIN_SEGMENT} mm, got {value}"
            ),
            format!(
                "{id}: a {} must be greater than {SPATIAL_MIN_SEGMENT} mm, got {value}",
                name.replace('_', " ")
            ),
        ));
    }
    Ok(())
}

/// The planar points and entities that stand for the spatial ones in
/// in-plane relations ([`SpatialSystem::in_plane_point`],
/// [`SpatialSystem::in_plane_curve`]), made as the relations need them, in
/// the plane square to `normal` (mitcad#1550).
#[derive(Debug, Clone)]
struct InPlaneIds {
    normal: SpatialAxis,
    /// A rotated plane's key in the solver (mitcad#1661), in place of
    /// `normal`.
    frame: Option<usize>,
    points: BTreeMap<EntityUid, PointId>,
    curves: BTreeMap<EntityUid, EntityId>,
    anchors: BTreeMap<Ref, SolverAnchor>,
    spacings: BTreeMap<ConstraintUid, (Vec<Ref>, SpacingMode, Option<EntityUid>)>,
}

impl InPlaneIds {
    fn new(normal: SpatialAxis) -> Self {
        Self {
            normal,
            frame: None,
            points: BTreeMap::new(),
            curves: BTreeMap::new(),
            anchors: BTreeMap::new(),
            spacings: BTreeMap::new(),
        }
    }
}

impl PlanarIds for InPlaneIds {
    fn point_id(&self, uid: EntityUid) -> Result<PointId, String> {
        self.points.get(&uid).copied().ok_or_else(|| {
            crate::said(
                "the sketch has no such point",
                format!("p{} is not a point of the sketch", uid.0),
            )
        })
    }

    fn curve_id(&self, uid: EntityUid) -> Result<EntityId, String> {
        self.curves.get(&uid).copied().ok_or_else(|| {
            crate::said(
                "in-plane relations take lines, and arcs, circles and full ellipses in their \
                 plane (splines in the sketch plane)",
                format!(
                    "c{} is not a line, an arc, circle or ellipse in the {} plane or a spline",
                    uid.0,
                    if self.frame.is_some() {
                        "rotated"
                    } else {
                        self.normal.plane_name()
                    }
                ),
            )
        })
    }

    fn anchor_of(&self, r: Ref) -> Result<SolverAnchor, String> {
        self.anchors.get(&r).copied().ok_or_else(|| not_spaced(r))
    }

    fn spacing_of(&self, constraint: ConstraintUid) -> Option<SpacingDef<'_>> {
        let (anchors, mode, direction) = self.spacings.get(&constraint)?;
        Some((anchors.as_slice(), *mode, *direction))
    }
}

/// What a solver row stands for: a relation or dimension (a plane-bound
/// ellipse's own rows are the solver's curve rows, mitcad#1550).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowOwner {
    Relation(ConstraintUid),
}

/// Why a 3D sketch did not solve, or an edit of one was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialFailureKind {
    /// The stored definition is inconsistent (an id, a coordinate, a
    /// relation a curve does not take).
    Definition,
    /// The support plane was not found.
    Support,
    /// An attachment's or reference curve's source was not found: missing,
    /// ambiguous, of a later feature or of another component.
    Reference,
    /// A driving dimension's parameter gives no valid value.
    Value,
    /// Relations contradict each other.
    Conflict,
    /// No solution was found from the stored positions, and no
    /// contradiction was proven.
    NotConverged,
    /// The relations hold only with invalid geometry: a collapsed line,
    /// radius, sweep or spline spacing, an alignment, normal or tangency
    /// pointing against its sense, an arc's midpoint off the arc.
    InvalidGeometry,
}

impl SpatialFailureKind {
    /// `definition`, `support`, `reference`, `value`, `conflict`,
    /// `not_converged` or `invalid_geometry`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::Support => "support",
            Self::Reference => "reference",
            Self::Value => "value",
            Self::Conflict => "conflict",
            Self::NotConverged => "not_converged",
            Self::InvalidGeometry => "invalid_geometry",
        }
    }
}

/// A failed solve or evaluation, with the relations and entities it is
/// about.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialFailure {
    pub kind: SpatialFailureKind,
    pub message: String,
    /// The relations and dimensions involved (conflicting, not solvable,
    /// a lost attachment, a bad value), ascending.
    pub constraints: Vec<ConstraintUid>,
    /// The points and curves involved (an attached point, a collapsed
    /// line or curve), ascending.
    pub entities: Vec<EntityUid>,
}

impl SpatialFailure {
    /// A failure of `message`: a text a person may read as it is, or one
    /// with its tail ([`FeatureError::text`], [`crate::said`]).
    pub fn new(kind: SpatialFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            constraints: Vec::new(),
            entities: Vec::new(),
        }
    }

    /// A failure that says `said` to a person, in plain words, and `named`
    /// (with the ids) to agents (mitcad#1644), about these relations and
    /// entities: they are its places in the sketch.
    pub fn said(
        kind: SpatialFailureKind,
        said: impl Into<String>,
        named: impl Into<String>,
        constraints: Vec<ConstraintUid>,
        entities: Vec<EntityUid>,
    ) -> Self {
        Self {
            kind,
            message: crate::said(said, named),
            constraints,
            entities,
        }
    }

    /// The failure as read: a person's message, its places of the model
    /// (a source's vertex, edge or face) and the agents' detail.
    pub fn error(&self) -> FeatureError {
        FeatureError::from_text(&self.message)
    }

    /// What a person reads (mitcad#1644): the message without ids.
    pub fn user_message(&self) -> String {
        self.error().message
    }

    /// What agents read: the message with the ids.
    pub fn agent_text(&self) -> String {
        self.error().agent_text().to_owned()
    }
}

impl fmt::Display for SpatialFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SpatialFailure {}

/// Degrees of freedom and what is determined after a successful solve.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialSketchStatus {
    /// Remaining degrees of freedom (redundant relations take none away).
    pub dof: usize,
    /// Solved, valid and without freedom left.
    pub fully_constrained: bool,
    /// Points whose three coordinates are determined, ascending.
    pub determined_points: Vec<EntityUid>,
    /// Lines whose two points are determined, ascending.
    pub determined_lines: Vec<EntityUid>,
    /// Arcs, circles and splines whose points and own unknowns (a normal,
    /// a radius, hidden control points) are determined, and ellipses whose
    /// points are, ascending.
    pub determined_curves: Vec<EntityUid>,
    /// Relations and dimensions that follow from others (consistent),
    /// ascending.
    pub redundant: Vec<ConstraintUid>,
    /// Every dimension's value at the solved geometry (driven ones too),
    /// millimetres (radians for an in-plane angle).
    pub measured: BTreeMap<ConstraintUid, f64>,
}

/// A solved curve's own geometry besides its points (local coordinates).
#[derive(Debug, Clone, PartialEq)]
pub enum SolvedCurve {
    /// An arc's or circle's unit normal and radius.
    Round { normal: [f64; 3], radius: f64 },
    /// A spline's degree, knots, weights and control net (a fit-point
    /// spline's hidden control points included).
    Spline(SpatialSplineGeometry),
    /// An ellipse's or elliptical arc's major radius (centre to major
    /// point) and solved minor radius (mitcad#1550).
    Ellipse { major: f64, minor: f64 },
}

/// The tangent `C'(t)` of a clamped spline at its start (`end` 0) or its
/// end (`end` 1), in its parameter: `p (P1 - P0) / (u_{p+1} - u_1)` and
/// `p (Pn - Pn-1) / (u_{n+p} - u_n)`, with the weights' ratio of a
/// rational one. None for a spline without a tangent there.
pub fn spline_end_tangent(spline: &SpatialSplineGeometry, end: usize) -> Option<[f64; 3]> {
    let p = spline.degree;
    let ctrl = &spline.control_points;
    let k = &spline.knots;
    let n = ctrl.len().checked_sub(1)?;
    if p == 0 || n == 0 || end > 1 || k.len() != ctrl.len() + p + 1 {
        return None;
    }
    let w = |i: usize| spline.weights.get(i).copied().unwrap_or(1.0);
    let (a, b, span, ratio) = if end == 0 {
        (ctrl[0], ctrl[1], k[p + 1] - k[1], w(1) / w(0))
    } else {
        (ctrl[n - 1], ctrl[n], k[n + p] - k[n], w(n - 1) / w(n))
    };
    if span <= 0.0 || !ratio.is_finite() {
        return None;
    }
    let f = p as f64 / span * ratio;
    let t = [(b[0] - a[0]) * f, (b[1] - a[1]) * f, (b[2] - a[2]) * f];
    t.iter().all(|v| v.is_finite()).then_some(t)
}

/// A soft goal of a drag on a fit-point spline's end tangent
/// (mitcad#1526): `end` 0 is its first point, 1 its last; `tangent` the
/// wanted `C'(t)` in the spline's parameter (direction and magnitude),
/// local coordinates. It moves where the end is free or held by a
/// tangency (then only the magnitude).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialTangentPull {
    pub curve: EntityUid,
    pub end: usize,
    pub tangent: [f64; 3],
}

/// The solved points and curves of a 3D sketch, in its local frame, and
/// its status.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialSolved {
    pub points: BTreeMap<EntityUid, [f64; 3]>,
    /// Arcs', circles' and splines' own geometry, by curve id.
    pub curves: BTreeMap<EntityUid, SolvedCurve>,
    /// Every tangency's sense.
    pub tangent_senses: BTreeMap<ConstraintUid, SpatialSense>,
    pub status: SpatialSketchStatus,
}

/// A definition's solver system with the ids that map between them
/// ([`SpatialSketchDef::build_with`]).
#[derive(Debug, Clone)]
pub struct SpatialModel {
    system: SpatialSystem,
    /// The sketch's point of each solver point, by index.
    point_owner: BTreeMap<usize, EntityUid>,
    point_of: BTreeMap<EntityUid, SpatialPointId>,
    /// The solver line or curve of each sketch curve (none for ellipses).
    curve_of: BTreeMap<EntityUid, SolverCurve>,
    line_owner: BTreeMap<usize, EntityUid>,
    curve_owner: BTreeMap<usize, EntityUid>,
    /// What each solver constraint stands for, by index.
    rows: BTreeMap<usize, RowOwner>,
    /// Each tangency's solver constraint.
    tangents: Vec<(ConstraintUid, SpatialConstraintId)>,
    /// Each On Face relation's solver constraint (mitcad#1549).
    faces: BTreeMap<ConstraintUid, SpatialConstraintId>,
    curves: Vec<SpatialCurve>,
    /// The in-plane relations of each frame plane (for measuring spacings).
    in_plane: BTreeMap<SpatialAxis, Vec<Constraint>>,
    dimensions: Vec<(ConstraintUid, SpatialDimensionKind)>,
    /// The planar stand-ins of each rotated plane's in-plane relations, by
    /// its key in the solver (mitcad#1661).
    frames: BTreeMap<usize, InPlaneIds>,
}

fn ids_text<T: fmt::Display>(ids: &[T]) -> String {
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn curve_name(id: EntityUid) -> String {
    format!("c{}", id.0)
}

/// The refusal of an equal spacing's anchor `r` that is no point, line,
/// circle or arc (mitcad#1644: the id for agents).
fn not_spaced(r: Ref) -> String {
    crate::said(
        "an equal spacing measures between points, lines, circles and arcs",
        format!("{r} is not a point, line, circle or arc to space"),
    )
}

impl SpatialModel {
    fn definition_failure(
        message: String,
        constraints: Vec<ConstraintUid>,
        entities: Vec<EntityUid>,
    ) -> SpatialFailure {
        SpatialFailure {
            kind: SpatialFailureKind::Definition,
            message,
            constraints,
            entities,
        }
    }

    /// The definition failure of `inner` (a text, perhaps with its tail)
    /// about the relation or entity `named` (mitcad#1644): a person reads
    /// inner's message, agents `named: ` before it.
    fn failure_about(
        named: impl fmt::Display,
        inner: &str,
        constraints: Vec<ConstraintUid>,
        entities: Vec<EntityUid>,
    ) -> SpatialFailure {
        Self::definition_failure(
            said_after("", &format!("{named}: "), inner),
            constraints,
            entities,
        )
    }

    fn point(&self, id: EntityUid) -> SpatialPointId {
        // check_definition found every point a curve or relation names.
        self.point_of[&id]
    }

    /// The solver curve of a sketch curve, for relation `owner`.
    fn solver_curve(
        &self,
        id: EntityUid,
        owner: ConstraintUid,
    ) -> Result<SolverCurve, SpatialFailure> {
        self.curve_of.get(&id).copied().ok_or_else(|| {
            Self::definition_failure(
                crate::said(
                    "relations take lines, arcs, circles, splines and reference curves",
                    format!(
                        "{owner}: c{} takes no relations (relations take lines, arcs, circles, \
                         splines and reference curves)",
                        id.0
                    ),
                ),
                vec![owner],
                vec![id],
            )
        })
    }

    /// The solver direction of a line or of an arc's or circle's normal,
    /// for relation `owner` (mitcad#1661).
    fn solver_dir(
        &self,
        id: EntityUid,
        owner: ConstraintUid,
    ) -> Result<SpatialDir, SpatialFailure> {
        let round = self.curves.iter().any(|c| c.id == id && c.is_round());
        match self.solver_curve(id, owner)? {
            SolverCurve::Line(l) => Ok(SpatialDir::Line(l)),
            SolverCurve::Curve(c) if round => Ok(SpatialDir::Normal(c)),
            SolverCurve::Curve(_) => Err(Self::definition_failure(
                crate::said(
                    "a direction is a line's or the normal of an arc or circle",
                    format!(
                        "{owner}: c{} is no line, arc or circle: a direction is a line's or the \
                         normal of an arc or circle",
                        id.0
                    ),
                ),
                vec![owner],
                vec![id],
            )),
        }
    }

    fn add_row(&mut self, c: SpatialConstraint, owner: RowOwner) -> Result<(), String> {
        let id = self.system.add_constraint(c).map_err(|e| e.to_string())?;
        self.rows.insert(id.index(), owner);
        Ok(())
    }

    fn add_curve(
        &mut self,
        c: &SpatialCurve,
        inputs: &SpatialInputs,
    ) -> Result<(), SpatialFailure> {
        let id = c.id;
        let failed = |e: String| Self::failure_about(curve_name(id), &e, Vec::new(), vec![id]);
        let normal = |n: [f64; 3], plane: Option<SpatialDirection>| match plane {
            Some(p) => p.solver(),
            None => SpatialNormal::Free(n),
        };
        let points: Vec<SpatialPointId> = c.points().iter().map(|p| self.point(*p)).collect();
        let added = match &c.kind {
            SpatialCurveKind::Line { .. } => {
                let line = self
                    .system
                    .add_line(points[0], points[1])
                    .map_err(|e| failed(e.to_string()))?;
                self.line_owner.insert(line.index(), id);
                SolverCurve::Line(line)
            }
            SpatialCurveKind::Arc {
                normal: n, plane, ..
            } => SolverCurve::Curve(
                self.system
                    .add_arc(points[0], points[1], points[2], normal(*n, *plane))
                    .map_err(|e| failed(e.to_string()))?,
            ),
            SpatialCurveKind::Circle {
                radius,
                normal: n,
                plane,
                ..
            } => SolverCurve::Curve(
                self.system
                    .add_circle(points[0], *radius, normal(*n, *plane))
                    .map_err(|e| failed(e.to_string()))?,
            ),
            SpatialCurveKind::Spline {
                degree,
                weights,
                knots,
                ..
            } => SolverCurve::Curve(
                self.system
                    .add_bspline(
                        *degree as usize,
                        &points,
                        (!weights.is_empty()).then_some(weights.as_slice()),
                        (!knots.is_empty()).then_some(knots.as_slice()),
                    )
                    .map_err(|e| failed(e.to_string()))?,
            ),
            SpatialCurveKind::FittedSpline { closed, ends, .. } => {
                let ends = if *closed {
                    [SplineEnd::Periodic; 2]
                } else {
                    ends.map(SpatialSplineEnd::solver)
                };
                SolverCurve::Curve(
                    self.system
                        .add_fitted_spline(&points, ends)
                        .map_err(|e| failed(e.to_string()))?,
                )
            }
            // On a rotated plane (mitcad#1661).
            SpatialCurveKind::Ellipse {
                minor_radius,
                frame: Some(frame),
                ..
            } => SolverCurve::Curve(
                self.system
                    .add_ellipse_along(points[0], points[1], *minor_radius, frame.normal)
                    .map_err(|e| failed(e.to_string()))?,
            ),
            // Plane-bound, its minor radius solved (mitcad#1550).
            SpatialCurveKind::Ellipse {
                minor_radius,
                normal,
                ..
            } => SolverCurve::Curve(
                self.system
                    .add_ellipse(points[0], points[1], *minor_radius, normal.solver(), None)
                    .map_err(|e| failed(e.to_string()))?,
            ),
            SpatialCurveKind::EllipticalArc {
                minor_radius,
                normal,
                ..
            } => SolverCurve::Curve(
                self.system
                    .add_ellipse(
                        points[0],
                        points[1],
                        *minor_radius,
                        normal.solver(),
                        Some([points[2], points[3]]),
                    )
                    .map_err(|e| failed(e.to_string()))?,
            ),
            SpatialCurveKind::Reference { source, .. } => {
                let Some(curve) = inputs.curves.get(&id) else {
                    return Err(SpatialFailure {
                        kind: SpatialFailureKind::Reference,
                        message: said_at(
                            "the source of a reference curve was not found",
                            format!("c{}: the reference curve of {source} was not found", id.0),
                            source.places(),
                        ),
                        constraints: Vec::new(),
                        entities: vec![id],
                    });
                };
                SolverCurve::Curve(
                    self.system
                        .add_reference_curve(curve.clone())
                        .map_err(|e| failed(e.to_string()))?,
                )
            }
        };
        if let SolverCurve::Curve(curve) = added {
            self.curve_owner.insert(curve.index(), id);
        }
        self.curve_of.insert(id, added);
        Ok(())
    }

    /// Makes the planar stand-ins (in `plane`'s plane) of the points and
    /// curves `refs` names.
    fn shadow_refs(&mut self, plane: &mut InPlaneIds, refs: &[Ref]) -> Result<(), String> {
        let axis = plane.normal.solver();
        // A rotated plane's stand-ins (mitcad#1661), else a frame plane's.
        let key = plane.frame.unwrap_or(axis.index());
        for r in refs {
            match *r {
                Ref::Point(u) => {
                    if plane.points.contains_key(&u) {
                        continue;
                    }
                    let p = *self.point_of.get(&u).ok_or_else(|| {
                        crate::said(
                            "the point does not exist",
                            format!("p{} does not exist", u.0),
                        )
                    })?;
                    let id = self
                        .system
                        .in_plane_point_in(p, key)
                        .map_err(|e| e.to_string())?;
                    plane.points.insert(u, id);
                }
                Ref::Curve(u) => {
                    if plane.curves.contains_key(&u) {
                        continue;
                    }
                    let c = *self.curve_of.get(&u).ok_or_else(|| {
                        crate::said(
                            "in-plane relations take lines, and arcs, circles and full ellipses \
                             in their plane (splines in the sketch plane)",
                            format!("c{} takes no in-plane relations", u.0),
                        )
                    })?;
                    let id = self
                        .system
                        .in_plane_curve_in(c, key)
                        .map_err(|e| e.to_string())?;
                    plane.curves.insert(u, id);
                }
            }
        }
        Ok(())
    }

    /// What an in-plane equal spacing measures from: a point, an arc's or
    /// circle's centre, a line's middle.
    fn shadow_anchor(&mut self, plane: &mut InPlaneIds, r: Ref) -> Result<(), String> {
        let not_spaced = || not_spaced(r);
        let anchor = match r {
            Ref::Point(u) => {
                self.shadow_refs(plane, &[r])?;
                SolverAnchor::Point(plane.points[&u])
            }
            Ref::Curve(u) => {
                let kind = self
                    .curves
                    .iter()
                    .find(|c| c.id == u)
                    .map(|c| c.kind.clone())
                    .ok_or_else(not_spaced)?;
                match kind {
                    SpatialCurveKind::Arc { center, .. }
                    | SpatialCurveKind::Circle { center, .. } => {
                        self.shadow_refs(plane, &[Ref::Point(center)])?;
                        SolverAnchor::Point(plane.points[&center])
                    }
                    SpatialCurveKind::Line { start, end } => {
                        self.shadow_refs(plane, &[Ref::Point(start), Ref::Point(end)])?;
                        SolverAnchor::Mid(plane.points[&start], plane.points[&end])
                    }
                    _ => return Err(not_spaced()),
                }
            }
        };
        plane.anchors.insert(r, anchor);
        Ok(())
    }

    fn add_in_plane(&mut self, row: PlanarRow, owner: RowOwner) -> Result<(), String> {
        let id = self.system.add_in_plane(row).map_err(|e| e.to_string())?;
        self.rows.insert(id.index(), owner);
        Ok(())
    }

    /// A kept pattern's or offset's binding as an in-plane row, owned by
    /// the pattern or offset (mitcad#1550).
    fn add_binding(&mut self, b: &Binding, plane: &mut InPlaneIds) -> Result<(), SpatialFailure> {
        let failed = |e: String| Self::failure_about(b.owner, &e, vec![b.owner], Vec::new());
        self.shadow_refs(plane, &binding_refs(&b.kind))
            .map_err(failed)?;
        let ids: &InPlaneIds = plane;
        let row = planar_binding(ids, &b.kind).map_err(failed)?;
        self.add_in_plane(row, RowOwner::Relation(b.owner))
            .map_err(failed)
    }

    fn add_relation(
        &mut self,
        c: &SpatialConstraintDef,
        inputs: &SpatialInputs,
        planes: &mut BTreeMap<SpatialAxis, InPlaneIds>,
    ) -> Result<(), SpatialFailure> {
        let owner = RowOwner::Relation(c.id);
        let failed = |e: String| Self::failure_about(c.id, &e, vec![c.id], Vec::new());
        let plane = planes
            .entry(c.kind.in_plane_normal().unwrap_or(SpatialAxis::Z))
            .or_insert_with_key(|axis| InPlaneIds::new(*axis));
        let rows: Vec<SpatialConstraint> = match &c.kind {
            SpatialConstraintKind::Coincident { a, b } => {
                vec![SpatialConstraint::Coincident(
                    self.point(*a),
                    self.point(*b),
                )]
            }
            SpatialConstraintKind::Fix { point, at } => vec![SpatialConstraint::Fix {
                point: self.point(*point),
                at: *at,
            }],
            SpatialConstraintKind::Attach { point, source, .. } => {
                let target = match (inputs.targets.get(&c.id), source) {
                    (Some(target), _) => *target,
                    (None, SpatialSource::Origin) => [0.0; 3],
                    (None, _) => {
                        return Err(SpatialFailure {
                            kind: SpatialFailureKind::Reference,
                            message: said_at(
                                "the source a point is attached to was not found",
                                format!(
                                    "{}: p{} is attached to {source}, which was not found",
                                    c.id, point.0
                                ),
                                source.places(),
                            ),
                            constraints: vec![c.id],
                            entities: vec![*point],
                        });
                    }
                };
                vec![SpatialConstraint::Attach {
                    point: self.point(*point),
                    target,
                }]
            }
            SpatialConstraintKind::Align { line, axis, sense } => {
                let SolverCurve::Line(l) = self.solver_curve(*line, c.id)? else {
                    return Err(failed(crate::said(
                        "an alignment takes a line",
                        format!("c{} is not a line", line.0),
                    )));
                };
                vec![SpatialConstraint::Align {
                    line: l,
                    axis: axis.solver(),
                    sense: sense.solver(),
                }]
            }
            SpatialConstraintKind::PointOn { point, curve } => vec![SpatialConstraint::PointOn {
                point: self.point(*point),
                curve: self.solver_curve(*curve, c.id)?,
            }],
            SpatialConstraintKind::Midpoint { point, curve } => {
                vec![SpatialConstraint::Midpoint {
                    point: self.point(*point),
                    curve: self.solver_curve(*curve, c.id)?,
                }]
            }
            SpatialConstraintKind::Tangent { a, b, at, sense } => {
                let tangent = SpatialConstraint::Tangent {
                    a: self.solver_curve(*a, c.id)?,
                    b: self.solver_curve(*b, c.id)?,
                    at: self.point(*at),
                };
                let id = self
                    .system
                    .add_constraint(tangent)
                    .map_err(|e| failed(e.to_string()))?;
                self.rows.insert(id.index(), owner);
                if let Some(sense) = sense {
                    self.system
                        .set_tangent_sense(id, sense.solver())
                        .map_err(|e| failed(e.to_string()))?;
                }
                self.tangents.push((c.id, id));
                return Ok(());
            }
            SpatialConstraintKind::Equal { a, b } => vec![SpatialConstraint::Equal(
                self.solver_curve(*a, c.id)?,
                self.solver_curve(*b, c.id)?,
            )],
            SpatialConstraintKind::NormalAlign { curve, axis, sense } => {
                let SolverCurve::Curve(id) = self.solver_curve(*curve, c.id)? else {
                    return Err(failed(crate::said(
                        "a normal alignment takes an arc or circle with a free normal",
                        format!("c{} is not an arc or circle", curve.0),
                    )));
                };
                vec![SpatialConstraint::NormalAlign {
                    curve: id,
                    axis: axis.solver(),
                    sense: sense.solver(),
                }]
            }
            SpatialConstraintKind::OnSketchPlane { points } => points
                .iter()
                .map(|p| SpatialConstraint::OnSketchPlane(self.point(*p)))
                .collect(),
            // The points' coordinates along the axis equal the first's.
            SpatialConstraintKind::PlaneBound { points, normal } => points
                .iter()
                .skip(1)
                .map(|p| SpatialConstraint::Offset {
                    a: self.point(points[0]),
                    b: self.point(*p),
                    axis: normal.solver(),
                    value: 0.0,
                })
                .collect(),
            SpatialConstraintKind::InPlane { relation, .. } => {
                if let ConstraintKind::EqualSpacing {
                    anchors,
                    mode,
                    direction,
                } = relation
                {
                    for r in anchors {
                        self.shadow_anchor(plane, *r).map_err(failed)?;
                    }
                    plane
                        .spacings
                        .insert(c.id, (anchors.clone(), *mode, *direction));
                }
                self.shadow_refs(plane, &relation.refs()).map_err(failed)?;
                let ids: &InPlaneIds = plane;
                for row in planar_constraint(ids, relation).map_err(failed)? {
                    self.add_in_plane(row, owner).map_err(failed)?;
                }
                return Ok(());
            }
            SpatialConstraintKind::OnFace { point, source } => {
                let Some(surface) = inputs.surfaces.get(&c.id) else {
                    return Err(SpatialFailure {
                        kind: SpatialFailureKind::Reference,
                        message: said_at(
                            "the face a point lies on was not found",
                            format!(
                                "{}: p{} lies on {source}, which was not found",
                                c.id, point.0
                            ),
                            source.places(),
                        ),
                        constraints: vec![c.id],
                        entities: vec![*point],
                    });
                };
                let id = self
                    .system
                    .add_constraint(SpatialConstraint::OnSurface {
                        point: self.point(*point),
                        surface: *surface,
                    })
                    .map_err(|e| failed(e.to_string()))?;
                self.rows.insert(id.index(), owner);
                self.faces.insert(c.id, id);
                return Ok(());
            }
            SpatialConstraintKind::Parallel { a, b } => vec![SpatialConstraint::Parallel(
                self.solver_dir(*a, c.id)?,
                self.solver_dir(*b, c.id)?,
            )],
            SpatialConstraintKind::Perpendicular { a, b } => {
                vec![SpatialConstraint::Perpendicular(
                    self.solver_dir(*a, c.id)?,
                    self.solver_dir(*b, c.id)?,
                )]
            }
            SpatialConstraintKind::Symmetric { a, b, plane } => {
                vec![SpatialConstraint::Symmetric {
                    a: self.point(*a),
                    b: self.point(*b),
                    plane: relation_plane(c, plane, inputs)?,
                }]
            }
            SpatialConstraintKind::OnPlane { points, plane } => {
                let surface = relation_plane(c, plane, inputs)?;
                points
                    .iter()
                    .map(|p| SpatialConstraint::OnSurface {
                        point: self.point(*p),
                        surface,
                    })
                    .collect()
            }
            SpatialConstraintKind::Concentric { a, b, normal } => {
                let (SolverCurve::Curve(ca), SolverCurve::Curve(cb)) =
                    (self.solver_curve(*a, c.id)?, self.solver_curve(*b, c.id)?)
                else {
                    return Err(failed(crate::said(
                        "concentric takes two arcs or circles",
                        format!("c{} or c{} is a line", a.0, b.0),
                    )));
                };
                vec![SpatialConstraint::Concentric {
                    a: ca,
                    b: cb,
                    normal: *normal,
                }]
            }
            SpatialConstraintKind::Curvature { a, b, at, sense } => {
                let curvature = SpatialConstraint::Curvature {
                    a: self.solver_curve(*a, c.id)?,
                    b: self.solver_curve(*b, c.id)?,
                    at: self.point(*at),
                };
                let id = self
                    .system
                    .add_constraint(curvature)
                    .map_err(|e| failed(e.to_string()))?;
                self.rows.insert(id.index(), owner);
                if let Some(sense) = sense {
                    self.system
                        .set_tangent_sense(id, sense.solver())
                        .map_err(|e| failed(e.to_string()))?;
                }
                self.tangents.push((c.id, id));
                return Ok(());
            }
            // A rotated plane's (mitcad#1661): its own stand-ins, solved on
            // the points' coordinates along the frame's x and y.
            SpatialConstraintKind::InFrame { relation, frame } => {
                let key = self
                    .system
                    .add_in_plane_frame(frame.x, frame.y())
                    .map_err(|e| failed(e.to_string()))?;
                let mut ids = self.frames.remove(&key).unwrap_or_else(|| {
                    let mut ids = InPlaneIds::new(SpatialAxis::Z);
                    ids.frame = Some(key);
                    ids
                });
                let made = self
                    .shadow_refs(&mut ids, &relation.refs())
                    .and_then(|()| planar_constraint(&ids, relation));
                let rows = match made {
                    Ok(rows) => rows,
                    Err(e) => {
                        self.frames.insert(key, ids);
                        return Err(failed(e));
                    }
                };
                self.frames.insert(key, ids);
                for row in rows {
                    self.add_in_plane(row, owner).map_err(failed)?;
                }
                return Ok(());
            }
            // The points' coordinates along the normal equal the first's.
            SpatialConstraintKind::FrameBound { points, frame } => points
                .iter()
                .skip(1)
                .map(|p| {
                    SpatialConstraint::Along(SpatialAlong::Offset {
                        a: self.point(points[0]),
                        b: self.point(*p),
                        dir: frame.normal,
                        value: 0.0,
                    })
                })
                .collect(),
            SpatialConstraintKind::AlignAlong {
                line,
                direction,
                sense,
            } => {
                let SolverCurve::Line(l) = self.solver_curve(*line, c.id)? else {
                    return Err(failed(crate::said(
                        "an alignment takes a line",
                        format!("c{} is not a line", line.0),
                    )));
                };
                vec![SpatialConstraint::Along(SpatialAlong::Line {
                    line: l,
                    dir: *direction,
                    sense: sense.solver(),
                })]
            }
            SpatialConstraintKind::NormalAlong { curve, direction } => {
                let SolverCurve::Curve(id) = self.solver_curve(*curve, c.id)? else {
                    return Err(failed(crate::said(
                        "a normal along a direction takes an arc or circle with a free normal",
                        format!("c{} is not an arc or circle", curve.0),
                    )));
                };
                vec![SpatialConstraint::Along(SpatialAlong::Normal {
                    curve: id,
                    dir: *direction,
                })]
            }
            SpatialConstraintKind::InPlaneFixed { entity } => {
                self.shadow_refs(plane, &[*entity]).map_err(failed)?;
                let row = match *entity {
                    Ref::Point(p) => PlanarRow::FixPoint(plane.point_id(p).map_err(failed)?),
                    Ref::Curve(e) => PlanarRow::FixEntity(plane.curve_id(e).map_err(failed)?),
                };
                return self.add_in_plane(row, owner).map_err(failed);
            }
        };
        for row in rows {
            self.add_row(row, owner).map_err(failed)?;
        }
        Ok(())
    }

    fn add_dimension(
        &mut self,
        id: ConstraintUid,
        kind: &SpatialDimensionKind,
        value: f64,
        planes: &mut BTreeMap<SpatialAxis, InPlaneIds>,
    ) -> Result<(), SpatialFailure> {
        let owner = RowOwner::Relation(id);
        let normal = match kind {
            SpatialDimensionKind::InPlane { normal, .. } => *normal,
            _ => SpatialAxis::Z,
        };
        let plane = planes
            .entry(normal)
            .or_insert_with_key(|axis| InPlaneIds::new(*axis));
        let failed = |message: String| SpatialFailure {
            kind: SpatialFailureKind::Value,
            message: said_after("", &format!("{id}: "), &message),
            constraints: vec![id],
            entities: Vec::new(),
        };
        let round = |model: &Self, curve: EntityUid| -> Result<SpatialCurveId, SpatialFailure> {
            match model.solver_curve(curve, id)? {
                SolverCurve::Curve(c) => Ok(c),
                SolverCurve::Line(_) => Err(Self::definition_failure(
                    crate::said(
                        "the dimension takes an arc or circle, not a line",
                        format!("{id}: c{} is a line, not an arc or circle", curve.0),
                    ),
                    vec![id],
                    vec![curve],
                )),
            }
        };
        let row = match kind {
            SpatialDimensionKind::Length { line } => {
                let SolverCurve::Line(l) = self.solver_curve(*line, id)? else {
                    return Err(failed(crate::said(
                        "a length dimension takes a line",
                        format!("c{} is not a line", line.0),
                    )));
                };
                SpatialConstraint::Length { line: l, value }
            }
            SpatialDimensionKind::Offset { a, b, axis } => SpatialConstraint::Offset {
                a: self.point(*a),
                b: self.point(*b),
                axis: axis.solver(),
                value,
            },
            SpatialDimensionKind::OffsetAlong { a, b, direction } => {
                SpatialConstraint::Along(SpatialAlong::Offset {
                    a: self.point(*a),
                    b: self.point(*b),
                    dir: *direction,
                    value,
                })
            }
            SpatialDimensionKind::Radius { curve } => SpatialConstraint::Radius {
                curve: round(self, *curve)?,
                value,
            },
            SpatialDimensionKind::Diameter { curve } => SpatialConstraint::Diameter {
                curve: round(self, *curve)?,
                value,
            },
            SpatialDimensionKind::ArcLength { curve } => SpatialConstraint::ArcLength {
                curve: round(self, *curve)?,
                value,
            },
            SpatialDimensionKind::SplineLength { curve } => SpatialConstraint::SplineLength {
                curve: round(self, *curve)?,
                value,
            },
            SpatialDimensionKind::TangentMagnitude { curve, end } => {
                SpatialConstraint::TangentLength {
                    curve: round(self, *curve)?,
                    end: end.index(),
                    value,
                }
            }
            SpatialDimensionKind::Angle { a, b } => SpatialConstraint::Angle {
                a: self.solver_dir(*a, id)?,
                b: self.solver_dir(*b, id)?,
                value,
            },
            SpatialDimensionKind::MajorRadius { curve } => SpatialConstraint::EllipseRadius {
                curve: round(self, *curve)?,
                minor: false,
                value,
            },
            SpatialDimensionKind::MinorRadius { curve } => SpatialConstraint::EllipseRadius {
                curve: round(self, *curve)?,
                minor: true,
                value,
            },
            SpatialDimensionKind::InPlane { dimension, .. } => {
                self.shadow_refs(plane, &dimension.refs()).map_err(failed)?;
                let ids: &InPlaneIds = plane;
                let row = planar_dimension(ids, dimension, value).map_err(failed)?;
                return self.add_in_plane(row, owner).map_err(failed);
            }
        };
        self.add_row(row, owner).map_err(failed)
    }

    /// Solves every relation from the stored positions. A failure leaves
    /// the model's points where they were.
    pub fn solve(&mut self) -> Result<SpatialSolved, SpatialFailure> {
        self.settle_hidden();
        let result = self.system.solve(&SpatialOptions::default());
        self.finish(&result)
    }

    /// Replaces the surface of the On Face relation `id` (mitcad#1549): the
    /// next plane touching a face that is solved through the kernel's
    /// closest-point projection. The next solve starts where the last one
    /// left the points.
    pub fn set_face_surface(
        &mut self,
        id: ConstraintUid,
        surface: SpatialSurface,
    ) -> Result<(), SpatialFailure> {
        let failed = |message: String| SpatialFailure {
            kind: SpatialFailureKind::Reference,
            message: said_after("", &format!("{id}: "), &message),
            constraints: vec![id],
            entities: Vec::new(),
        };
        let solver = *self
            .faces
            .get(&id)
            .ok_or_else(|| failed("no On Face relation".to_owned()))?;
        self.system
            .set_surface(solver, surface)
            .map_err(|e| failed(e.to_string()))
    }

    /// Fits the hidden control points of the fit-point splines to their
    /// fit points where they are, before the solve (mitcad#1522). Each build
    /// makes them by interpolation with natural ends, which a tangency at an
    /// end does not keep; the solve's correction from there, spread over
    /// every unknown by its weight, would move free fit points too (a few
    /// hundredths of a millimetre), on every solve again and after a drag.
    /// So the fit points are held for a first solve that moves the rest;
    /// when that one does not converge (a relation that only moving a fit
    /// point satisfies), it leaves the system as it was and the solve
    /// starts from the interpolation as before.
    fn settle_hidden(&mut self) {
        let fit: BTreeSet<EntityUid> = self
            .curves
            .iter()
            .flat_map(|c| match &c.kind {
                SpatialCurveKind::FittedSpline { points, .. } => points.clone(),
                _ => Vec::new(),
            })
            .collect();
        if fit.is_empty() {
            return;
        }
        let mut holds = Vec::with_capacity(fit.len());
        for p in &fit {
            let Some(&id) = self.point_of.get(p) else {
                continue;
            };
            let Some(at) = self.system.point(id) else {
                continue;
            };
            if let Ok(hold) = self
                .system
                .add_constraint(SpatialConstraint::Fix { point: id, at })
            {
                holds.push(hold);
            }
        }
        // The stored end tangents of free and tangent ends (mitcad#1526):
        // the interpolation's natural ends would lose a dragged handle.
        let tangents = self.stored_tangents();
        // Converged or not, nothing of it but the positions is kept.
        if tangents.is_empty() {
            let _ = self.system.solve(&SpatialOptions::default());
        } else {
            let _ = self
                .system
                .drag_with(&[], &tangents, &SpatialOptions::default());
        }
        for hold in holds {
            let _ = self.system.remove_constraint(hold);
        }
    }

    /// The solver's goals for the end tangents the open fit-point splines
    /// store.
    fn stored_tangents(&self) -> Vec<SpatialTangentGoal> {
        let mut goals = Vec::new();
        for curve in &self.curves {
            let SpatialCurveKind::FittedSpline {
                closed: false,
                tangents,
                ..
            } = &curve.kind
            else {
                continue;
            };
            let Some(SolverCurve::Curve(c)) = self.curve_of.get(&curve.id) else {
                continue;
            };
            for (end, tangent) in tangents.iter().enumerate() {
                if let Some(t) = tangent
                    && t.iter().all(|v| v.is_finite())
                {
                    goals.push(SpatialTangentGoal {
                        spline: *c,
                        end,
                        target: *t,
                    });
                }
            }
        }
        goals
    }

    /// Pulls points toward local targets as far as the relations allow
    /// (temporary soft goals: nothing is stored as a relation).
    pub fn drag(
        &mut self,
        goals: &[(EntityUid, [f64; 3])],
    ) -> Result<SpatialSolved, SpatialFailure> {
        self.drag_with(goals, &[])
    }

    /// [`SpatialModel::drag`] with fit-point splines' end tangents pulled
    /// too (mitcad#1526: a tangent handle's drag).
    pub fn drag_with(
        &mut self,
        goals: &[(EntityUid, [f64; 3])],
        tangents: &[SpatialTangentPull],
    ) -> Result<SpatialSolved, SpatialFailure> {
        let mut handles = Vec::with_capacity(tangents.len());
        for pull in tangents {
            let open = self.curves.iter().any(|c| {
                c.id == pull.curve
                    && matches!(c.kind, SpatialCurveKind::FittedSpline { closed: false, .. })
            });
            let solver = match self.curve_of.get(&pull.curve) {
                Some(SolverCurve::Curve(c)) if open => *c,
                _ => {
                    return Err(SpatialFailure::said(
                        SpatialFailureKind::Definition,
                        "only an open fit-point spline has tangent handles",
                        format!(
                            "c{} is no open fit-point spline: only those have tangent handles",
                            pull.curve.0
                        ),
                        Vec::new(),
                        vec![pull.curve],
                    ));
                }
            };
            handles.push(SpatialTangentGoal {
                spline: solver,
                end: pull.end,
                target: pull.tangent,
            });
        }
        let mut pulls = Vec::with_capacity(goals.len());
        for (point, target) in goals {
            let Some(id) = self.point_of.get(point) else {
                return Err(SpatialFailure::said(
                    SpatialFailureKind::Definition,
                    "the point does not exist",
                    format!("point p{} does not exist", point.0),
                    Vec::new(),
                    Vec::new(),
                ));
            };
            pulls.push(SpatialDragGoal {
                point: *id,
                target: *target,
            });
        }
        // The other fit points stay where they are (mitcad#1522), and so do
        // the stored end tangents not pulled (mitcad#1526).
        self.settle_hidden();
        for kept in self.stored_tangents() {
            if !handles
                .iter()
                .any(|h| h.spline == kept.spline && h.end == kept.end)
            {
                handles.push(kept);
            }
        }
        let result = self
            .system
            .drag_with(&pulls, &handles, &SpatialOptions::default())
            .map_err(|e| SpatialFailure::new(SpatialFailureKind::Definition, e.to_string()))?;
        self.finish(&result)
    }

    /// The relation of a solver constraint, added to `constraints`.
    fn owner_into(&self, id: SpatialConstraintId, constraints: &mut Vec<ConstraintUid>) {
        if let Some(RowOwner::Relation(k)) = self.rows.get(&id.index()) {
            constraints.push(*k);
        }
    }

    /// The relations of solver constraints, ascending, and the curves they
    /// are about (none: a curve's own rows are its geometry, mitcad#1550).
    fn owners(
        &self,
        ids: impl IntoIterator<Item = SpatialConstraintId>,
    ) -> (Vec<ConstraintUid>, Vec<EntityUid>) {
        let mut constraints = Vec::new();
        for id in ids {
            self.owner_into(id, &mut constraints);
        }
        sort_dedup(&mut constraints);
        (constraints, Vec::new())
    }

    fn finish(&mut self, result: &SpatialSolveResult) -> Result<SpatialSolved, SpatialFailure> {
        if !result.is_ok() {
            return Err(self.failure(result));
        }
        let analysis = self.system.analyze();
        if !analysis.conflicts.is_empty() || !analysis.issues.is_empty() {
            let (mut constraints, mut entities) = self.owners(analysis.conflicting_constraints());
            let (more, more_entities) = self.issues(&analysis.issues);
            constraints.extend(more);
            entities.extend(more_entities);
            sort_dedup(&mut constraints);
            sort_dedup(&mut entities);
            let kind = if analysis.conflicts.is_empty() {
                SpatialFailureKind::InvalidGeometry
            } else {
                SpatialFailureKind::Conflict
            };
            let mut names: Vec<String> = constraints.iter().map(ToString::to_string).collect();
            names.extend(entities.iter().map(|e| curve_name(*e)));
            // The relations and curves are its places (mitcad#1644).
            return Err(SpatialFailure::said(
                kind,
                "the solved geometry is not valid",
                format!("the solved geometry is not valid: {}", names.join(", ")),
                constraints,
                entities,
            ));
        }
        let points: BTreeMap<EntityUid, [f64; 3]> = self
            .point_of
            .iter()
            .map(|(id, p)| (*id, self.system.point(*p).unwrap_or_default()))
            .collect();
        let mut curves = BTreeMap::new();
        for (uid, curve) in &self.curve_of {
            let SolverCurve::Curve(c) = *curve else {
                continue;
            };
            let solved = match self.system.curve_kind(c) {
                Some(SpatialEntityKind::Arc | SpatialEntityKind::Circle) => self
                    .system
                    .normal(c)
                    .zip(self.system.radius(c))
                    .map(|(normal, radius)| SolvedCurve::Round { normal, radius }),
                Some(SpatialEntityKind::BSpline | SpatialEntityKind::FittedSpline) => {
                    self.system.spline(c).map(SolvedCurve::Spline)
                }
                Some(SpatialEntityKind::Ellipse) => self
                    .system
                    .ellipse_radii(c)
                    .map(|(major, minor)| SolvedCurve::Ellipse { major, minor }),
                _ => None,
            };
            if let Some(solved) = solved {
                curves.insert(*uid, solved);
            }
        }
        let tangent_senses: BTreeMap<ConstraintUid, SpatialSense> = self
            .tangents
            .iter()
            .filter_map(|(k, id)| {
                Some((*k, SpatialSense::of_solver(self.system.tangent_sense(*id)?)))
            })
            .collect();
        let (redundant, _) = self.owners(analysis.redundant_constraints());
        let measured = {
            let at = |id: EntityUid| points.get(&id).copied();
            let round = |id: EntityUid| match curves.get(&id) {
                Some(SolvedCurve::Round { normal, radius }) => Some((*normal, *radius)),
                _ => None,
            };
            let minor = |id: EntityUid| match curves.get(&id) {
                Some(SolvedCurve::Ellipse { minor, .. }) => Some(*minor),
                _ => None,
            };
            let geometry = Geometry {
                curves: &self.curves,
                at: &at,
                round: &round,
                minor: &minor,
            };
            let no_relations = Vec::new();
            self.dimensions
                .iter()
                .filter_map(|(id, kind)| {
                    let value = match kind {
                        SpatialDimensionKind::SplineLength { curve } => {
                            match self.curve_of.get(curve) {
                                Some(SolverCurve::Curve(c)) => self.system.spline_length(*c),
                                _ => None,
                            }
                        }
                        SpatialDimensionKind::TangentMagnitude { curve, end } => {
                            match self.curve_of.get(curve) {
                                Some(SolverCurve::Curve(c)) => {
                                    self.system.tangent_length(*c, end.index())
                                }
                                _ => None,
                            }
                        }
                        SpatialDimensionKind::InPlane { normal, .. } => geometry
                            .measure(kind, self.in_plane.get(normal).unwrap_or(&no_relations)),
                        _ => geometry.measure(kind, &no_relations),
                    }?;
                    Some((*id, value))
                })
                .collect()
        };
        let determined_points = sorted(
            analysis
                .fully_constrained_points
                .iter()
                .filter_map(|p| self.point_owner.get(&p.index()).copied()),
        );
        let determined_lines = sorted(
            analysis
                .fully_constrained_lines
                .iter()
                .filter_map(|l| self.line_owner.get(&l.index()).copied()),
        );
        // An ellipse's minor radius is one of its own unknowns (mitcad#1550).
        let determined_curves = sorted(
            analysis
                .fully_constrained_curves
                .iter()
                .filter_map(|c| self.curve_owner.get(&c.index()).copied())
                .filter(|id| self.curves.iter().any(|c| c.id == *id && !c.is_reference())),
        );
        let status = SpatialSketchStatus {
            dof: analysis.dof,
            fully_constrained: analysis.is_fully_constrained(),
            determined_points,
            determined_lines,
            determined_curves,
            redundant,
            measured,
        };
        Ok(SpatialSolved {
            points,
            curves,
            tangent_senses,
            status,
        })
    }

    /// The relations and curves of invalid geometry and singular rows,
    /// ascending.
    fn issues(&self, issues: &[SpatialIssue]) -> (Vec<ConstraintUid>, Vec<EntityUid>) {
        let mut constraints = Vec::new();
        let mut entities = Vec::new();
        for issue in issues {
            match *issue {
                SpatialIssue::CollapsedLine(l) => {
                    entities.extend(self.line_owner.get(&l.index()).copied());
                }
                SpatialIssue::ReversedSense { constraint, line } => {
                    self.owner_into(constraint, &mut constraints);
                    entities.extend(self.line_owner.get(&line.index()).copied());
                }
                SpatialIssue::Singular(c)
                | SpatialIssue::ReversedTangent(c)
                | SpatialIssue::MidpointOffArc(c) => {
                    self.owner_into(c, &mut constraints);
                }
                SpatialIssue::DegenerateCurve(c) => {
                    entities.extend(self.curve_owner.get(&c.index()).copied());
                }
                SpatialIssue::ReversedNormal { constraint, curve } => {
                    self.owner_into(constraint, &mut constraints);
                    entities.extend(self.curve_owner.get(&curve.index()).copied());
                }
            }
        }
        sort_dedup(&mut constraints);
        sort_dedup(&mut entities);
        (constraints, entities)
    }

    fn failure(&self, result: &SpatialSolveResult) -> SpatialFailure {
        let (issue_constraints, entities) = self.issues(&result.issues);
        match result.status {
            SpatialStatus::Conflicting => {
                let (mut constraints, mut involved) = self.owners(result.conflicting());
                constraints.extend(issue_constraints);
                involved.extend(entities);
                sort_dedup(&mut constraints);
                sort_dedup(&mut involved);
                let mut names: Vec<String> = constraints.iter().map(ToString::to_string).collect();
                names.extend(
                    involved
                        .iter()
                        .map(|e| format!("the plane of {}", curve_name(*e))),
                );
                // The relations and the curves whose planes take part are
                // its places; a person reads words (mitcad#1644).
                let said = if involved.is_empty() {
                    "relations of the sketch contradict each other"
                } else {
                    "relations of the sketch contradict each other and the planes of curves"
                };
                SpatialFailure::said(
                    SpatialFailureKind::Conflict,
                    said,
                    format!("the relations {} contradict each other", names.join(", ")),
                    constraints,
                    involved,
                )
            }
            SpatialStatus::Invalid => {
                let curves: Vec<String> = entities.iter().map(|l| curve_name(*l)).collect();
                let mut parts = Vec::new();
                if !curves.is_empty() {
                    parts.push(format!(
                        "{} would have no length or extent (a length, radius, sweep or spacing \
                         of at most {SPATIAL_MIN_SEGMENT} mm)",
                        curves.join(", ")
                    ));
                }
                if !issue_constraints.is_empty() {
                    parts.push(format!(
                        "{} would point against its sense or leave its side",
                        ids_text(&issue_constraints)
                    ));
                }
                // One whole message per case (mitcad#1644).
                let said = match (curves.is_empty(), issue_constraints.is_empty()) {
                    (false, true) => format!(
                        "the relations hold only with invalid geometry: a curve would have no \
                         length or extent (a length, radius, sweep or spacing of at most \
                         {SPATIAL_MIN_SEGMENT} mm)"
                    ),
                    (true, false) => "the relations hold only with invalid geometry: a relation \
                                      would point against its sense or leave its side"
                        .to_owned(),
                    (false, false) => format!(
                        "the relations hold only with invalid geometry: a curve would have no \
                         length or extent (a length, radius, sweep or spacing of at most \
                         {SPATIAL_MIN_SEGMENT} mm), and a relation would point against its sense \
                         or leave its side"
                    ),
                    (true, true) => {
                        "the relations hold only with invalid geometry: a collapsed curve"
                            .to_owned()
                    }
                };
                SpatialFailure::said(
                    SpatialFailureKind::InvalidGeometry,
                    said,
                    format!("the relations hold only with invalid geometry: {}", {
                        if parts.is_empty() {
                            "a collapsed curve".to_owned()
                        } else {
                            parts.join("; ")
                        }
                    }),
                    issue_constraints,
                    entities,
                )
            }
            SpatialStatus::NotConverged | SpatialStatus::Converged => {
                let detail = if issue_constraints.is_empty() {
                    String::new()
                } else {
                    format!(
                        " ({} cannot be solved where its curve has no length or extent)",
                        ids_text(&issue_constraints)
                    )
                };
                let said = if issue_constraints.is_empty() {
                    format!(
                        "no solution was found from the current positions; the largest error is \
                         {:.3e} mm",
                        result.residual
                    )
                } else {
                    format!(
                        "no solution was found from the current positions (a relation cannot be \
                         solved where its curve has no length or extent); the largest error is \
                         {:.3e} mm",
                        result.residual
                    )
                };
                SpatialFailure::said(
                    SpatialFailureKind::NotConverged,
                    said,
                    format!(
                        "no solution was found from the current positions{detail}; the \
                         largest error is {:.3e} mm",
                        result.residual
                    ),
                    issue_constraints,
                    entities,
                )
            }
        }
    }
}

/// The plane of a Symmetric or On Plane relation in local coordinates
/// (mitcad#1661): a frame plane's own, another found before the solve
/// ([`SpatialInputs::surfaces`]).
fn relation_plane(
    c: &SpatialConstraintDef,
    plane: &SpatialPlaneRef,
    inputs: &SpatialInputs,
) -> Result<SpatialSurface, SpatialFailure> {
    if let SpatialPlaneRef::Frame(axis) = plane {
        return Ok(SpatialPlaneRef::frame_surface(*axis));
    }
    inputs
        .surfaces
        .get(&c.id)
        .copied()
        .ok_or_else(|| SpatialFailure {
            kind: SpatialFailureKind::Reference,
            message: said_at(
                "the plane of a relation was not found",
                format!("{}: {plane} was not found", c.id),
                plane.places(),
            ),
            constraints: vec![c.id],
            entities: c.kind.points(),
        })
}

fn sort_dedup<T: Ord>(v: &mut Vec<T>) {
    v.sort_unstable();
    v.dedup();
}

fn sorted(ids: impl Iterator<Item = EntityUid>) -> Vec<EntityUid> {
    let mut ids: Vec<EntityUid> = ids.collect();
    sort_dedup(&mut ids);
    ids
}

/// The frame of a 3D sketch from its support's frame: the same origin,
/// unit `x`, `y` made at right angles to `x`, and `n = x × y`. An error for
/// a plane without two directions.
pub fn orthonormal_frame(frame: &SketchFrame) -> Result<SketchFrame, String> {
    let degenerate = || "the support plane has no frame (its axes are parallel)".to_owned();
    let x = unit(frame.x_axis).ok_or_else(degenerate)?;
    let along = dot(frame.y_axis, x);
    let y = unit([
        frame.y_axis[0] - along * x[0],
        frame.y_axis[1] - along * x[1],
        frame.y_axis[2] - along * x[2],
    ])
    .ok_or_else(degenerate)?;
    if !finite3(frame.origin) {
        return Err("the support plane's origin is not finite".to_owned());
    }
    Ok(SketchFrame {
        origin: frame.origin,
        x_axis: x,
        y_axis: y,
    })
}

/// Local coordinates of a component point: `R^T (V - O)`.
pub fn to_local(frame: &SketchFrame, point: [f64; 3]) -> [f64; 3] {
    direction_to_local(frame, sub(point, frame.origin))
}

/// The component point of local coordinates: `O + R p`.
pub fn to_component(frame: &SketchFrame, local: [f64; 3]) -> [f64; 3] {
    let d = direction_to_component(frame, local);
    std::array::from_fn(|i| frame.origin[i] + d[i])
}

/// A component direction in local coordinates: `R^T v`.
pub fn direction_to_local(frame: &SketchFrame, v: [f64; 3]) -> [f64; 3] {
    [
        dot(v, frame.x_axis),
        dot(v, frame.y_axis),
        dot(v, cross(frame.x_axis, frame.y_axis)),
    ]
}

/// A local direction in the component's coordinates: `R v`.
pub fn direction_to_component(frame: &SketchFrame, v: [f64; 3]) -> [f64; 3] {
    let n = cross(frame.x_axis, frame.y_axis);
    std::array::from_fn(|i| v[0] * frame.x_axis[i] + v[1] * frame.y_axis[i] + v[2] * n[i])
}

/// A model curve of the sketch's component as constant geometry in its
/// local frame (`R^T (C - O)`): the form a source edge, another sketch's
/// curve or an intersection curve takes as a reference curve (mitcad#1543,
/// [`SpatialInputs::curves`]). None for a point.
pub fn local_reference_curve(frame: &SketchFrame, curve: &Curve3) -> Option<SpatialReferenceCurve> {
    let point = |p: [f64; 3]| to_local(frame, p);
    let turn = |v: [f64; 3]| direction_to_local(frame, v);
    Some(match curve {
        Curve3::Point(_) => return None,
        Curve3::Line { start, end } => SpatialReferenceCurve::Line {
            start: point(*start),
            end: point(*end),
        },
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => SpatialReferenceCurve::Conic {
            center: point(*center),
            normal: turn(*normal),
            x_axis: turn(*x_axis),
            major: *major,
            minor: *minor,
            start: *start,
            end: *end,
            closed: *closed,
        },
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => SpatialReferenceCurve::BSpline {
            degree: *degree as usize,
            poles: poles.iter().map(|p| point(*p)).collect(),
            weights: weights.clone(),
            knots: knots.clone(),
        },
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => SpatialReferenceCurve::Helix {
            center: point(*center),
            axis: turn(*axis),
            x_axis: turn(*x_axis),
            radius: *radius,
            pitch: *pitch,
            start: *start,
            end: *end,
        },
    })
}

/// The local target of a point source (mitcad#1543) from its geometry in
/// the component's coordinates: a point's own place, or the start, end or
/// middle (`at`) of a curve ([`SpatialReferenceCurve::ends_and_middle`]: the
/// middle halfway along it by length); with `projected` along the sketch's
/// normal onto its plane (local `z = 0`, Project).
pub fn local_source_point(
    frame: &SketchFrame,
    geometry: &Curve3,
    at: Option<SpatialCurvePoint>,
    projected: bool,
) -> Result<[f64; 3], String> {
    let local = match (geometry, at) {
        (Curve3::Point(p), None) => to_local(frame, *p),
        // One whole message per point of a curve (mitcad#1644).
        (Curve3::Point(_), Some(SpatialCurvePoint::Start)) => {
            return Err("it is a point: it has no start".to_owned());
        }
        (Curve3::Point(_), Some(SpatialCurvePoint::End)) => {
            return Err("it is a point: it has no end".to_owned());
        }
        (Curve3::Point(_), Some(SpatialCurvePoint::Middle)) => {
            return Err("it is a point: it has no middle".to_owned());
        }
        (_, None) => {
            return Err(
                "it is a curve: a point lies on its reference curve, or is attached to its \
                 start, end or middle"
                    .to_owned(),
            );
        }
        (curve, Some(at)) => {
            let local = local_reference_curve(frame, curve).ok_or("it is not a curve")?;
            let [start, end, middle] = local
                .ends_and_middle()?
                .ok_or("it is a closed curve: it has no start, end or middle")?;
            match at {
                SpatialCurvePoint::Start => start,
                SpatialCurvePoint::End => end,
                SpatialCurvePoint::Middle => middle,
            }
        }
    };
    Ok(if projected {
        [local[0], local[1], 0.0]
    } else {
        local
    })
}

/// A face's surface (as the kernel describes it in the sketch's component,
/// [`SurfaceGeometry`]) in the sketch's local frame (mitcad#1549,
/// [`SpatialInputs::surfaces`]): a plane, cylinder, cone, sphere or torus,
/// solved in closed form. None for another kind of surface, which a point
/// is put on through the kernel's closest-point projection
/// ([`touching_plane`]), and for a surface without valid values.
pub fn local_surface(frame: &SketchFrame, surface: &SurfaceGeometry) -> Option<SpatialSurface> {
    let point = |p: [f64; 3]| to_local(frame, p);
    let turn = |v: [f64; 3]| unit(direction_to_local(frame, v));
    let local = match *surface {
        SurfaceGeometry::Plane { origin, normal } => SpatialSurface::Plane {
            origin: point(origin),
            normal: turn(normal)?,
        },
        SurfaceGeometry::Cylinder {
            origin,
            axis,
            radius,
        } => SpatialSurface::Cylinder {
            origin: point(origin),
            axis: turn(axis)?,
            radius,
        },
        SurfaceGeometry::Cone {
            origin,
            axis,
            radius,
            half_angle,
        } => SpatialSurface::Cone {
            origin: point(origin),
            axis: turn(axis)?,
            radius,
            half_angle,
        },
        SurfaceGeometry::Sphere { center, radius } => SpatialSurface::Sphere {
            center: point(center),
            radius,
        },
        SurfaceGeometry::Torus {
            center,
            axis,
            major_radius,
            minor_radius,
        } => SpatialSurface::Torus {
            center: point(center),
            axis: turn(axis)?,
            major: major_radius,
            minor: minor_radius,
        },
        SurfaceGeometry::Other { .. } => return None,
    };
    local.check().is_ok().then_some(local)
}

/// The plane touching a face at its point `at` with the normal `normal`
/// there (the kernel's closest-point projection, in the component's
/// coordinates), in the sketch's local frame: the surface a point on a
/// face of another kind is solved on, replaced until the point lies on
/// the face (mitcad#1549). None for a normal of no length.
pub fn touching_plane(
    frame: &SketchFrame,
    at: [f64; 3],
    normal: [f64; 3],
) -> Option<SpatialSurface> {
    Some(SpatialSurface::Plane {
        origin: to_local(frame, at),
        normal: unit(direction_to_local(frame, normal))?,
    })
}

/// A unit vector at right angles to the unit `n`.
fn across(n: [f64; 3]) -> Option<[f64; 3]> {
    let from = |a: [f64; 3]| {
        let along = dot(a, n);
        unit([
            a[0] - along * n[0],
            a[1] - along * n[1],
            a[2] - along * n[2],
        ])
    };
    if n[0].abs() < 0.9 {
        from([1.0, 0.0, 0.0])
    } else {
        from([0.0, 1.0, 0.0])
    }
}

/// What evaluating a 3D sketch gave: its frame, its solved points and
/// curves and their status, or why it failed. A failed evaluation keeps no
/// geometry (`solved` is None), so nothing takes the last successful
/// result for the current one.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialSketchOutput {
    /// The orthonormal support frame in the component's coordinates; None
    /// when the support was not found.
    pub frame: Option<SketchFrame>,
    /// The curves of the definition evaluated, by id.
    pub curves: BTreeMap<EntityUid, SpatialCurve>,
    /// Each attachment's target in local coordinates, as resolved before
    /// the solve (the origin's is zero).
    pub targets: BTreeMap<ConstraintUid, [f64; 3]>,
    /// Each reference curve's geometry in local coordinates, as resolved
    /// before the solve.
    pub references: BTreeMap<EntityUid, SpatialReferenceCurve>,
    /// Each On Face relation's surface in local coordinates (mitcad#1549),
    /// as the solve last used it: an analytic face's surface, or the plane
    /// touching another kind of face at the solved point; and each
    /// Symmetric or On Plane relation's plane found outside the sketch
    /// (mitcad#1661).
    pub surfaces: BTreeMap<ConstraintUid, SpatialSurface>,
    /// The solve; None unless the evaluation succeeded.
    pub solved: Option<SpatialSolved>,
    /// Why it failed; None when it succeeded.
    pub failure: Option<SpatialFailure>,
}

impl SpatialSketchOutput {
    /// A failed evaluation of `def`.
    pub fn failed(
        frame: Option<SketchFrame>,
        def: &SpatialSketchDef,
        failure: SpatialFailure,
    ) -> Self {
        Self {
            frame,
            curves: def.curves.iter().map(|c| (c.id, c.clone())).collect(),
            targets: BTreeMap::new(),
            references: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            solved: None,
            failure: Some(failure),
        }
    }

    /// What the evaluation built the sketch with: its attachments' targets,
    /// reference curves and faces' surfaces (to build it again, e.g. for a
    /// drag).
    pub fn inputs(&self) -> SpatialInputs {
        SpatialInputs {
            targets: self.targets.clone(),
            curves: self.references.clone(),
            surfaces: self.surfaces.clone(),
        }
    }

    /// Solved with a frame and no failure.
    pub fn is_solved(&self) -> bool {
        self.failure.is_none() && self.solved.is_some() && self.frame.is_some()
    }

    /// A solved point in local coordinates.
    pub fn local_point(&self, id: EntityUid) -> Option<[f64; 3]> {
        self.solved.as_ref()?.points.get(&id).copied()
    }

    /// A solved point in the component's coordinates.
    pub fn point(&self, id: EntityUid) -> Option<[f64; 3]> {
        Some(to_component(self.frame.as_ref()?, self.local_point(id)?))
    }

    /// A solved curve as a model curve in the component's coordinates, once:
    /// a line; an arc as a conic (`major = minor = radius`, `x_axis` toward
    /// its start, `normal` its normal, from 0 to its sweep), a circle as a
    /// closed one; a spline as a B-spline (a fit-point spline by its solved
    /// control net); a plane-bound ellipse or elliptical arc as a conic in
    /// its plane. None for a reference curve (no path piece) and for an
    /// unsolved sketch.
    pub fn curve(&self, id: EntityUid) -> Option<Curve3> {
        self.curve_in(id, self.frame.as_ref()?)
    }

    /// A solved curve as [`SpatialSketchOutput::curve`] gives it, in the
    /// sketch's local coordinates (mitcad#1550: its planar form).
    pub fn local_curve(&self, id: EntityUid) -> Option<Curve3> {
        self.frame.as_ref()?;
        self.curve_in(id, &SketchFrame::XY)
    }

    /// A solved curve with `frame` as the sketch's frame.
    fn curve_in(&self, id: EntityUid, frame: &SketchFrame) -> Option<Curve3> {
        let solved = self.solved.as_ref()?;
        let local = |p: EntityUid| solved.points.get(&p).copied();
        let place = |p: [f64; 3]| to_component(frame, p);
        let turn = |v: [f64; 3]| direction_to_component(frame, v);
        let round = || match solved.curves.get(&id) {
            Some(SolvedCurve::Round { normal, radius }) => Some((*normal, *radius)),
            _ => None,
        };
        match &self.curves.get(&id)?.kind {
            SpatialCurveKind::Line { start, end } => Some(Curve3::Line {
                start: place(local(*start)?),
                end: place(local(*end)?),
            }),
            SpatialCurveKind::Arc {
                center, start, end, ..
            } => {
                let (normal, radius) = round()?;
                let c = local(*center)?;
                let (s, e) = (sub(local(*start)?, c), sub(local(*end)?, c));
                Some(Curve3::Conic {
                    center: place(c),
                    normal: turn(normal),
                    x_axis: turn(unit(s)?),
                    major: radius,
                    minor: radius,
                    start: 0.0,
                    end: sweep_about(normal, s, e),
                    closed: false,
                })
            }
            SpatialCurveKind::Circle { center, .. } => {
                let (normal, radius) = round()?;
                Some(Curve3::Conic {
                    center: place(local(*center)?),
                    normal: turn(normal),
                    x_axis: turn(across(normal)?),
                    major: radius,
                    minor: radius,
                    start: 0.0,
                    end: TAU,
                    closed: true,
                })
            }
            SpatialCurveKind::Spline { .. } | SpatialCurveKind::FittedSpline { .. } => {
                let Some(SolvedCurve::Spline(spline)) = solved.curves.get(&id) else {
                    return None;
                };
                let rational = spline.weights.iter().any(|w| (w - 1.0).abs() > 1e-12);
                Some(Curve3::BSpline {
                    degree: spline.degree as u32,
                    poles: spline.control_points.iter().map(|p| place(*p)).collect(),
                    weights: if rational {
                        spline.weights.clone()
                    } else {
                        Vec::new()
                    },
                    knots: spline.knots.clone(),
                })
            }
            SpatialCurveKind::Ellipse {
                center,
                major,
                minor_radius,
                normal,
                frame,
            } => {
                let c = local(*center)?;
                let m = sub(local(*major)?, c);
                let minor = match solved.curves.get(&id) {
                    Some(SolvedCurve::Ellipse { minor, .. }) => *minor,
                    _ => *minor_radius,
                };
                // A rotated plane's normal (mitcad#1661), else the axis'.
                let n = frame.map_or_else(|| normal.place([0.0, 0.0], 1.0), |f| f.normal);
                Some(Curve3::Conic {
                    center: place(c),
                    normal: turn(n),
                    x_axis: turn(unit(m)?),
                    major: norm(m),
                    minor,
                    start: 0.0,
                    end: TAU,
                    closed: true,
                })
            }
            SpatialCurveKind::EllipticalArc {
                center,
                major,
                minor_radius,
                start,
                end,
                normal,
            } => {
                let c = local(*center)?;
                let m = sub(local(*major)?, c);
                let (a, u) = (norm(m), unit(m)?);
                let n = normal.place([0.0, 0.0], 1.0);
                let v = cross(n, u);
                let minor_radius = match solved.curves.get(&id) {
                    Some(SolvedCurve::Ellipse { minor, .. }) => *minor,
                    _ => *minor_radius,
                };
                let param = |p: [f64; 3]| {
                    let d = sub(p, c);
                    (dot(d, v) / minor_radius).atan2(dot(d, u) / a)
                };
                let t0 = param(local(*start)?);
                let mut t1 = param(local(*end)?);
                while t1 <= t0 {
                    t1 += TAU;
                }
                Some(Curve3::Conic {
                    center: place(c),
                    normal: turn(n),
                    x_axis: turn(u),
                    major: a,
                    minor: minor_radius,
                    start: t0,
                    end: t1,
                    closed: false,
                })
            }
            SpatialCurveKind::Reference { .. } => None,
        }
    }

    /// A point or curve of the sketch as a model curve in the component's
    /// coordinates, as a later sketch projects or includes it (mitcad#1543):
    /// a point as [`Curve3::Point`], a curve as [`SpatialSketchOutput::curve`]
    /// gives it. None for a reference curve, an entity the sketch does not
    /// have, and an unsolved sketch.
    pub fn entity_curve(&self, entity: Ref) -> Option<Curve3> {
        match entity {
            Ref::Point(id) => self.point(id).map(Curve3::Point),
            Ref::Curve(id) => self.curve(id),
        }
    }

    /// Every solved curve as a model curve, in id order; construction
    /// curves too (their flag is in [`SpatialSketchOutput::curves`]), not
    /// reference curves.
    pub fn model_curves(&self) -> Vec<(EntityUid, Curve3)> {
        self.curves
            .keys()
            .filter_map(|id| Some((*id, self.curve(*id)?)))
            .collect()
    }

    /// The status of the last successful solve.
    pub fn status(&self) -> Option<&SpatialSketchStatus> {
        self.solved.as_ref().map(|s| &s.status)
    }
}

/// A text of a 3D sketch with its contours, each a polyline of local points
/// ([`SpatialSketchDef::text_outlines`]).
pub type TextOutline = (EntityUid, Vec<Vec<[f64; 3]>>);
