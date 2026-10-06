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
    /// Equal length (lines) or equal radius (circles and arcs).
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
    /// Horizontal distance between two points.
    HorizontalDistance {
        a: PointId,
        b: PointId,
        value: f64,
    },
    /// Vertical distance between two points.
    VerticalDistance {
        a: PointId,
        b: PointId,
        value: f64,
    },
    /// Length of a line.
    Length {
        line: EntityId,
        value: f64,
    },
    /// Angle between two lines in radians, in the quadrant where the lines
    /// are when the dimension is added (the one whose angle is closest to
    /// `value`).
    Angle {
        a: EntityId,
        b: EntityId,
        value: f64,
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
}

impl Constraint {
    /// Target value of a dimension, `None` for geometric constraints.
    pub fn value(&self) -> Option<f64> {
        use Constraint::*;
        match *self {
            Distance { value, .. }
            | PointLineDistance { value, .. }
            | LineDistance { value, .. }
            | HorizontalDistance { value, .. }
            | VerticalDistance { value, .. }
            | Length { value, .. }
            | Angle { value, .. }
            | Radius { value, .. }
            | Diameter { value, .. }
            | ArcLength { value, .. }
            | MajorRadius { value, .. }
            | MinorRadius { value, .. } => Some(value),
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
            | HorizontalDistance { value, .. }
            | VerticalDistance { value, .. }
            | Length { value, .. }
            | Angle { value, .. }
            | Radius { value, .. }
            | Diameter { value, .. }
            | ArcLength { value, .. }
            | MajorRadius { value, .. }
            | MinorRadius { value, .. } => *value = v,
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
