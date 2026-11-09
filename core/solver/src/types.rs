// SPDX-License-Identifier: MIT
//! Public identifiers, constraint definitions, options and results.

use std::fmt;

/// A point of the sketch. Entities refer to points, and entities that share a
/// point are joined there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PointId(pub(crate) u32);

/// A curve: line, circle, arc, ellipse, elliptical arc or spline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(pub(crate) u32);

/// A constraint or dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConstraintId(pub(crate) u32);

impl PointId {
    /// Index in creation order (stable for the lifetime of the system).
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl EntityId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl ConstraintId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Kind of an entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Line,
    Circle,
    /// Counter-clockwise from the start point to the end point.
    Arc,
    Ellipse,
    /// Counter-clockwise (in the ellipse parameter) from start to end.
    EllipticalArc,
    /// B-spline defined by control points (degree, knots and weights fixed).
    BSpline,
    /// Cubic spline interpolating fit points.
    FittedSpline,
}

/// End condition of a fitted spline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SplineEnd {
    /// Zero second derivative (a natural spline end). Dropped automatically
    /// when a tangent constraint controls this end, which leaves the tangent
    /// magnitude free.
    #[default]
    Natural,
    /// The end tangent is free (two degrees of freedom: direction and
    /// magnitude), to be constrained by the caller.
    Free,
    /// A closed (periodic) spline (mitcad#783), given for both ends: the
    /// curve runs on from the last fit point back to the first and has the
    /// same first and second derivatives at both ends (C2 through the
    /// first fit point). The fit points are listed once, at least three.
    Periodic,
}

/// Where an equal spacing measures from: a point (a sketch point, the centre
/// of a circle or an arc), or the middle of two points (the midpoint of a
/// line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Point(PointId),
    Mid(PointId, PointId),
}

impl Anchor {
    /// The points the anchor is made of.
    pub fn points(self) -> Vec<PointId> {
        match self {
            Anchor::Point(p) => vec![p],
            Anchor::Mid(p, q) => vec![p, q],
        }
    }
}

/// What the intervals of an equal spacing measure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpacingMode {
    /// The x offset of successive anchors (their y is left free).
    Horizontal,
    /// The y offset of successive anchors (their x is left free).
    Vertical,
    /// The offset projected on the direction of a line (the offset across
    /// it is left free).
    Along(EntityId),
    /// The distance between successive anchors.
    PointToPoint,
}

/// A constraint or a dimension. Dimensions carry a target `value` in
/// millimetres or radians, which [`crate::System::set_dimension_value`]
/// changes later.
///
/// Orientation choices (the side of a point-line distance, the side of a
/// tangency, the quadrant of an angle, internal or external tangency of
/// circles) are taken from the geometry when the constraint is added and
/// kept afterwards, so dimension changes do not mirror the geometry.
#[derive(Clone, Debug, PartialEq)]
pub enum Constraint {
    /// Two points at the same place.
    Coincident(PointId, PointId),
    /// A point on a curve: the infinite line, the full circle of an arc, the
    /// full ellipse of an elliptical arc, or the parameter range of a spline.
    PointOnCurve(PointId, EntityId),
    /// A horizontal line.
    Horizontal(EntityId),
    /// A vertical line.
    Vertical(EntityId),
    /// Two points on a horizontal line.
    HorizontalPoints(PointId, PointId),
    /// Two points on a vertical line.
    VerticalPoints(PointId, PointId),
    Parallel(EntityId, EntityId),
    Perpendicular(EntityId, EntityId),
    /// Tangency between a line, circle, arc or spline and a circle, arc or
    /// spline (or a line and a spline). When the curves share a point (a
    /// common end point, a coincident constraint or point-on-curve
    /// constraints), the tangency is at that point (a G1 join).
    Tangent(EntityId, EntityId),
    /// Curvature continuity (G2) where a spline joins a line, an arc or
    /// another spline at a shared end point: tangent there and with equal
    /// curvature (zero for a line). The splines need clamped knots.
    Smooth(EntityId, EntityId),
    /// Equal length (lines), equal radius (circles and arcs) or equal minor
    /// radius (ellipses and elliptical arcs).
    Equal(EntityId, EntityId),
    /// The point does not move (its position at the start of each solve).
    FixPoint(PointId),
    /// All points and radii of the entity stay where they are.
    FixEntity(EntityId),
    /// The point at the midpoint of a line or of an arc.
    Midpoint(PointId, EntityId),
    /// Circles, arcs, ellipses or elliptical arcs with the same centre.
    Concentric(EntityId, EntityId),
    /// Two lines on the same infinite line.
    Collinear(EntityId, EntityId),
    /// Two points mirrored about a line.
    SymmetricPoints {
        a: PointId,
        b: PointId,
        axis: EntityId,
    },
    /// Two lines, circles or arcs mirrored about a line.
    SymmetricEntities {
        a: EntityId,
        b: EntityId,
        axis: EntityId,
    },
    /// Two circles or arcs mirrored about a line with their radii left to
    /// other constraints (mitcad#865): the centres mirrored, and an arc's
    /// ends mirrored in direction from its centre. Two lines as
    /// [`Constraint::SymmetricEntities`].
    SymmetricCentres {
        a: EntityId,
        b: EntityId,
        axis: EntityId,
    },
    /// Distance between two points.
    Distance {
        a: PointId,
        b: PointId,
        value: f64,
    },
    /// Distance of a point from an (infinite) line.
    PointLineDistance {
        point: PointId,
        line: EntityId,
        value: f64,
    },
    /// Distance between two lines (meant for parallel lines): the distance of
    /// the midpoint of `b` from the line `a`.
    LineDistance {
        a: EntityId,
        b: EntityId,
        value: f64,
    },
    /// The gap between a circle (or arc) and a line (meant for a line the
    /// circle does not cross): the distance of its centre from the line
    /// minus its radius.
    TangentDistance {
        circle: EntityId,
        line: EntityId,
        value: f64,
    },
    /// The gap between two circles or arcs (meant for concentric ones): the
    /// difference of their radii, on the side `b` is when the dimension is
    /// added (mitcad#445).
    RadiusGap {
        a: EntityId,
        b: EntityId,
        value: f64,
    },
    /// The gap between two circles or arcs at their nearest sides
    /// (mitcad#1540): for circles outside each other `|c_b - c_a| - r_a -
    /// r_b`; with `inside`, one within the other, the larger radius (when
    /// the dimension is added) minus the smaller one minus `|c_b - c_a|`.
    TangentCircleDistance {
        a: EntityId,
        b: EntityId,
        value: f64,
        inside: bool,
    },
    /// The gap between a circle or arc and a point at the circle's nearest
    /// side (mitcad#1540): `|p - c| - r` for a point outside it; with
    /// `inside`, `r - |p - c|`.
    TangentPointDistance {
        circle: EntityId,
        point: PointId,
        value: f64,
        inside: bool,
    },
    /// Horizontal distance between two points: `|b.x - a.x|`, on the side
    /// `b` is when the dimension is added; with `signed` (mitcad#404),
    /// `b.x - a.x` itself, which may be negative.
    HorizontalDistance {
        a: PointId,
        b: PointId,
        value: f64,
        signed: bool,
    },
    /// Vertical distance between two points: as [`Self::HorizontalDistance`]
    /// in y.
    VerticalDistance {
        a: PointId,
        b: PointId,
        value: f64,
        signed: bool,
    },
    /// Length of a line.
    Length {
        line: EntityId,
        value: f64,
    },
    /// Angle between two lines in radians. `supplement` says which angle:
    /// `Some(false)` the one between their directions (start to end),
    /// `Some(true)` its supplement (between `a`'s direction and `b`'s
    /// reversed), `None` the one of the two closest to `value` where the
    /// lines are when the dimension is added.
    Angle {
        a: EntityId,
        b: EntityId,
        value: f64,
        supplement: Option<bool>,
    },
    /// Radius of a circle or arc.
    Radius {
        entity: EntityId,
        value: f64,
    },
    /// Diameter of a circle or arc.
    Diameter {
        entity: EntityId,
        value: f64,
    },
    /// Length of an arc.
    ArcLength {
        arc: EntityId,
        value: f64,
    },
    /// Major radius of an ellipse or elliptical arc.
    MajorRadius {
        ellipse: EntityId,
        value: f64,
    },
    /// Minor radius of an ellipse or elliptical arc.
    MinorRadius {
        ellipse: EntityId,
        value: f64,
    },
    // Copies of sketch patterns: the copy's points follow the original's.
    /// `b` is `a` moved by `by`.
    Translated {
        a: PointId,
        b: PointId,
        by: [f64; 2],
    },
    /// `b` is `a` turned by `angle` (radians, counter-clockwise) about
    /// `center`.
    Rotated {
        center: PointId,
        a: PointId,
        b: PointId,
        angle: f64,
    },
    /// The direction from `b_center` to `b` is the direction from
    /// `a_center` to `a` turned by `angle`: one equation, for a point its
    /// curve already keeps at its distance from the centre (the end of an
    /// arc, the ends of an elliptical arc).
    TurnedDirection {
        a_center: PointId,
        a: PointId,
        b_center: PointId,
        b: PointId,
        angle: f64,
    },
    /// The size the points of a circle or an ellipse leave free: equal
    /// radii of circles, equal minor radii of ellipses and elliptical arcs.
    EqualSize(EntityId, EntityId),
    // Equal spacing (mitcad#221).
    /// Successive intervals between the anchors, in the order given, are
    /// equal: `interval(a[i], a[i+1]) = interval(a[i+1], a[i+2])` measured
    /// by `mode` (signed for the directional modes, so the order holds),
    /// n - 2 equations for n anchors. Their common size stays free; a
    /// [`Constraint::Spacing`] drives it. Nothing else of the anchors is
    /// constrained (no alignment).
    EqualSpacing {
        anchors: Vec<Anchor>,
        mode: SpacingMode,
    },
    /// The interval from `a` to `b` measured by `mode`: the common spacing
    /// of an equal spacing whose first two anchors they are. Directional
    /// modes measure it the way the anchors are apart when it is added.
    Spacing {
        a: Anchor,
        b: Anchor,
        mode: SpacingMode,
        value: f64,
    },
    // Tangency at a chosen end (mitcad#783).
    /// [`Constraint::Tangent`] at the end of each curve that is at `at` (by
    /// coincidence, else the nearest end), so that a curve can be tangent at
    /// both ends to different curves, or to the same curve at both ends.
    /// Lines, circles and arcs touch there when the curves share the point.
    /// `a` and `b` may be the same open spline whose two ends are at `at`:
    /// the spline is then smooth (G1) where it closes.
    TangentAt {
        a: EntityId,
        b: EntityId,
        at: PointId,
    },
    /// [`Constraint::Smooth`] at the end of each curve that is at `at`, as
    /// [`Constraint::TangentAt`].
    SmoothAt {
        a: EntityId,
        b: EntityId,
        at: PointId,
    },
}

impl Constraint {
    /// Target value of a dimension, `None` for geometric constraints.
    pub fn value(&self) -> Option<f64> {
        use Constraint::*;
        match *self {
            Distance { value, .. }
            | PointLineDistance { value, .. }
            | LineDistance { value, .. }
            | TangentDistance { value, .. }
            | RadiusGap { value, .. }
            | TangentCircleDistance { value, .. }
            | TangentPointDistance { value, .. }
            | HorizontalDistance { value, .. }
            | VerticalDistance { value, .. }
            | Length { value, .. }
            | Angle { value, .. }
            | Radius { value, .. }
            | Diameter { value, .. }
            | ArcLength { value, .. }
            | MajorRadius { value, .. }
            | MinorRadius { value, .. }
            | Spacing { value, .. } => Some(value),
            _ => None,
        }
    }

    pub fn is_dimension(&self) -> bool {
        self.value().is_some()
    }

    pub(crate) fn set_value(&mut self, v: f64) {
        use Constraint::*;
        match self {
            Distance { value, .. }
            | PointLineDistance { value, .. }
            | LineDistance { value, .. }
            | TangentDistance { value, .. }
            | RadiusGap { value, .. }
            | TangentCircleDistance { value, .. }
            | TangentPointDistance { value, .. }
            | HorizontalDistance { value, .. }
            | VerticalDistance { value, .. }
            | Length { value, .. }
            | Angle { value, .. }
            | Radius { value, .. }
            | Diameter { value, .. }
            | ArcLength { value, .. }
            | MajorRadius { value, .. }
            | MinorRadius { value, .. }
            | Spacing { value, .. } => *value = v,
            _ => {}
        }
    }

    /// True for dimensions measured as an angle (radians).
    pub(crate) fn is_angular(&self) -> bool {
        matches!(self, Constraint::Angle { .. })
    }
}

/// Errors of the editing operations.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    UnknownPoint(PointId),
    UnknownEntity(EntityId),
    UnknownConstraint(ConstraintId),
    /// The entity has the wrong kind for this use.
    WrongEntityKind {
        entity: EntityId,
        expected: &'static str,
    },
    /// A value or definition is out of range (negative length, too few
    /// control points, ...).
    InvalidValue(&'static str),
    /// The constraint is not a dimension.
    NotADimension(ConstraintId),
    /// The combination is not supported.
    Unsupported(&'static str),
    /// The point or entity is still used by an entity or constraint.
    InUse,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnknownPoint(p) => write!(f, "unknown point {}", p.0),
            Error::UnknownEntity(e) => write!(f, "unknown entity {}", e.0),
            Error::UnknownConstraint(c) => write!(f, "unknown constraint {}", c.0),
            Error::WrongEntityKind { entity, expected } => {
                write!(f, "entity {} is not {expected}", entity.0)
            }
            Error::InvalidValue(what) => write!(f, "invalid value: {what}"),
            Error::NotADimension(c) => write!(f, "constraint {} is not a dimension", c.0),
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
            Error::InUse => write!(f, "still in use"),
        }
    }
}

impl std::error::Error for Error {}

/// Options of [`crate::System::solve`] and [`crate::System::drag`].
#[derive(Clone, Debug)]
pub struct SolveOptions {
    /// Convergence tolerance relative to the sketch size: every residual
    /// (in millimetres, angles scaled by the sketch size) must be below
    /// `tolerance * size`.
    pub tolerance: f64,
    /// Iteration limit of one Newton/Levenberg-Marquardt run.
    pub max_iterations: usize,
    /// Apply large dimension changes in steps (continuation), which keeps the
    /// geometry on the branch it started on.
    pub continuation: bool,
}

impl Default for SolveOptions {
    fn default() -> Self {
        SolveOptions {
            tolerance: 1e-9,
            max_iterations: 100,
            continuation: true,
        }
    }
}

/// Outcome of a solve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolveStatus {
    /// All constraints hold within the tolerance.
    Converged,
    /// Some constraints contradict each other; see [`SolveResult::conflicts`].
    /// The geometry of the affected parts is left where it was.
    Conflicting,
    /// No solution was found from the current geometry, although no
    /// contradiction was detected (for example, a distance larger than the
    /// geometry allows). The affected parts are left where they were.
    NotConverged,
}

/// A constraint that depends on others, with the set of constraints involved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    /// The constraint that is redundant or over-constrains (the most recently
    /// added one of the set).
    pub constraint: ConstraintId,
    /// All constraints of the dependent set, `constraint` included, ascending.
    pub involved: Vec<ConstraintId>,
}

/// Result of [`crate::System::solve`] or [`crate::System::drag`].
#[derive(Clone, Debug, PartialEq)]
pub struct SolveResult {
    pub status: SolveStatus,
    /// Iterations of the longest run (over independent parts and steps).
    pub iterations: usize,
    /// Largest remaining residual in millimetres.
    pub residual: f64,
    /// Contradicting constraint sets when the status is `Conflicting`.
    pub conflicts: Vec<Dependency>,
    /// Lines that collapsed to a point or circles and arcs whose radius
    /// became zero (a warning; the constraints hold).
    pub degenerate: Vec<EntityId>,
}

impl SolveResult {
    pub fn is_ok(&self) -> bool {
        self.status == SolveStatus::Converged
    }

    /// All constraints of all conflicting sets, ascending.
    pub fn conflicting(&self) -> Vec<ConstraintId> {
        let mut v: Vec<ConstraintId> = self
            .conflicts
            .iter()
            .flat_map(|d| d.involved.iter().copied())
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// A soft goal for [`crate::System::drag`]: pulled toward its target as far as
/// the constraints allow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragGoal {
    Point {
        point: PointId,
        target: [f64; 2],
    },
    /// Radius of a circle (arcs: drag an end point instead).
    Radius {
        entity: EntityId,
        target: f64,
    },
}

/// Degrees of freedom and constraint status of the sketch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    /// Remaining degrees of freedom (0: fully constrained).
    pub dof: usize,
    /// Points whose position is fully determined, ascending.
    pub fully_constrained_points: Vec<PointId>,
    /// Entities whose points and radii are all determined, ascending.
    pub fully_constrained_entities: Vec<EntityId>,
    /// Constraints that follow from the others (consistent).
    pub redundant: Vec<Dependency>,
    /// Constraints that contradict the others.
    pub conflicts: Vec<Dependency>,
}

impl Analysis {
    pub fn is_point_fully_constrained(&self, p: PointId) -> bool {
        self.fully_constrained_points.binary_search(&p).is_ok()
    }

    pub fn is_entity_fully_constrained(&self, e: EntityId) -> bool {
        self.fully_constrained_entities.binary_search(&e).is_ok()
    }

    /// Ids of the redundant constraints, ascending.
    pub fn redundant_constraints(&self) -> Vec<ConstraintId> {
        let mut v: Vec<ConstraintId> = self.redundant.iter().map(|d| d.constraint).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// All constraints of all conflicting sets, ascending.
    pub fn conflicting_constraints(&self) -> Vec<ConstraintId> {
        let mut v: Vec<ConstraintId> = self
            .conflicts
            .iter()
            .flat_map(|d| d.involved.iter().copied())
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}
