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
//! The solver is our own implementation (MIT) of well-known methods; see
//! `README.md` for the formulation. It has no dependencies.
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
mod direction;
mod equations;
mod prepare;
mod scalar;
mod solve;
mod sparse;
mod spline;
mod system;
mod types;

pub use system::{SplineGeometry, System};
pub use types::{
    Analysis, Constraint, ConstraintId, Dependency, DragGoal, EntityId, EntityKind, Error, PointId,
    SolveOptions, SolveResult, SolveStatus, SplineEnd,
};
