// SPDX-License-Identifier: MIT
//! Mitcad 2D sketch constraint solver.
//!
//! Geometry (points, lines, circles, arcs, ellipses, elliptical arcs,
//! B-splines and fitted splines) lives in sketch plane coordinates
//! (millimetres, radians). Constraints and dimensions become residual
//! equations; [`System::solve`] moves the geometry as little as possible
//! until all of them hold, [`System::drag`] pulls points toward a target while
//! they hold, and [`System::analyze`] reports degrees of freedom, fully
//! constrained geometry and redundant or conflicting constraints.
//!
//! [`RigidSystem`] solves the joints of assemblies with the same methods:
//! rigid bodies joined by joints with free motions, limits and driven
//! values.
//!
//! [`SpatialSystem`] solves the points, lines, arcs, circles and splines of
//! 3D sketches (three coordinates per point) with the same numerical
//! engine, separate from the planar [`System`]; a converted sketch's planar
//! relations keep their planar rows on the x and y columns.
//!
//! The solver is our own implementation (MIT) of well-known methods; see
//! `README.md` for the formulation. It has no third-party dependencies.
//!
//! ```
//! use mitcad_solver::{Constraint, SolveOptions, System};
//!
//! let mut s = System::new();
//! // A rough rectangle: shared corner points join the lines.
//! let p = [
//!     s.add_point(0.0, 0.0),
//!     s.add_point(9.0, 0.5),
//!     s.add_point(10.0, 6.0),
//!     s.add_point(-0.5, 5.0),
//! ];
//! let l: Vec<_> = (0..4).map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap()).collect();
//! s.add_constraint(Constraint::FixPoint(p[0])).unwrap();
//! s.add_constraint(Constraint::Horizontal(l[0])).unwrap();
//! s.add_constraint(Constraint::Horizontal(l[2])).unwrap();
//! s.add_constraint(Constraint::Vertical(l[1])).unwrap();
//! s.add_constraint(Constraint::Vertical(l[3])).unwrap();
//! let width = s.add_constraint(Constraint::Length { line: l[0], value: 40.0 }).unwrap();
//! s.add_constraint(Constraint::Length { line: l[1], value: 20.0 }).unwrap();
//!
//! assert!(s.solve(&SolveOptions::default()).is_ok());
//! let c = s.point(p[2]).unwrap();
//! assert!((c[0] - 40.0).abs() < 1e-9 && (c[1] - 20.0).abs() < 1e-9);
//! assert_eq!(s.analyze().dof, 0);
//!
//! // Change a dimension and solve again.
//! s.set_dimension_value(width, 55.0).unwrap();
//! assert!(s.solve(&SolveOptions::default()).is_ok());
//! assert!((s.point(p[2]).unwrap()[0] - 55.0).abs() < 1e-9);
//! ```

mod analysis;
mod analysis_plan;
mod direction;
mod equations;
mod prepare;
mod rigid;
mod scalar;
mod solve;
mod sparse;
mod spatial;
mod spline;
mod system;
mod types;

pub use rigid::{
    Amount, BodyId, ConstraintPart, Freedom, JointId, JointMotion, Pose, RigidAnalysis,
    RigidConstraint, RigidDependency, RigidError, RigidJoint, RigidOptions, RigidResult,
    RigidSystem, Sense, motion_pose,
};
pub use system::{SplineGeometry, System};
pub use types::{
    Analysis, Constraint, ConstraintId, Dependency, DragGoal, EntityId, EntityKind, Error, PointId,
    SolveOptions, SolveResult, SolveStatus, SplineEnd,
};
// Equal spacing (mitcad#221).
pub use types::{Anchor, SpacingMode};
// Couplings of joints' motions (mitcad#466).
pub use rigid::{Relation, RigidCoupling};
// Spatial point/line system of 3D sketches (mitcad#1520).
pub use spatial::{
    Axis, AxisSense, SPATIAL_MIN_SEGMENT, SPATIAL_TOLERANCE, SpatialAnalysis, SpatialConstraint,
    SpatialConstraintId, SpatialDependency, SpatialDragGoal, SpatialError, SpatialIssue,
    SpatialLineId, SpatialOptions, SpatialPointId, SpatialSolveResult, SpatialStatus,
    SpatialSystem,
};
// Spatial arcs, circles, splines, reference curves and in-plane relations
// (mitcad#1542).
pub use spatial::{
    SpatialCircle, SpatialCurve, SpatialCurveId, SpatialEntityKind, SpatialNormal,
    SpatialReferenceCurve, SpatialSplineGeometry, SpatialTangentGoal, circle_through,
};
// Points on faces of bodies (mitcad#1549).
pub use spatial::SpatialSurface;
// Directions of parallel, perpendicular and angle relations (mitcad#1661).
pub use spatial::SpatialDir;
// Relations along constant directions: a rotated placement plane's axes and
// normal (mitcad#1661).
pub use spatial::SpatialAlong;
