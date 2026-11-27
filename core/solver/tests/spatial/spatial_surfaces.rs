// SPDX-License-Identifier: MIT
//! Points on surfaces of the spatial system (mitcad#1549): a point on a
//! plane, a cylinder, a cone, a sphere and a torus solved in closed form
//! with their degrees of freedom, a moved surface followed, a redundant
//! and a contradictory relation reported as for the other relations.

use std::f64::consts::FRAC_PI_4;

use mitcad_solver::AxisSense::Positive;
use mitcad_solver::SpatialConstraint::*;
use mitcad_solver::{
    Axis, SpatialError, SpatialOptions, SpatialPointId, SpatialStatus, SpatialSurface,
    SpatialSystem,
};

/// Geometric comparison tolerance (mm).
const EPS: f64 = 1e-6;
const Z: [f64; 3] = [0.0, 0.0, 1.0];

fn solve_ok(s: &mut SpatialSystem) {
    let r = s.solve(&SpatialOptions::default());
    assert!(r.is_ok(), "solve failed: {r:?}");
}

fn pt(s: &mut SpatialSystem, v: [f64; 3]) -> SpatialPointId {
    s.add_point(v).unwrap()
}

fn fix(s: &mut SpatialSystem, p: SpatialPointId) {
    let at = s.point(p).expect("point");
    s.add_constraint(Fix { point: p, at }).unwrap();
}

fn near(v: [f64; 3], want: [f64; 3]) -> bool {
    (0..3).all(|k| (v[k] - want[k]).abs() < EPS)
}

/// A free point on a plane keeps two degrees of freedom and moves onto it
/// as little as possible: straight across.
#[test]
fn point_on_a_plane_slides_on_it() {
    let mut s = SpatialSystem::new();
    let p = pt(&mut s, [3.0, 4.0, 7.0]);
    let plane = SpatialSurface::Plane {
        origin: [0.0, 0.0, 5.0],
        normal: Z,
    };
    s.add_constraint(OnSurface {
        point: p,
        surface: plane,
    })
    .unwrap();
    solve_ok(&mut s);
    assert!(near(s.point(p).unwrap(), [3.0, 4.0, 5.0]));
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (3, 1, 2));
    assert!(a.redundant.is_empty() && a.conflicts.is_empty());
}

/// A line from a fixed point along X ends on a cylinder about Z: the end
/// is determined (DOF 0), and follows the cylinder when its radius grows.
#[test]
fn line_end_on_a_cylinder_is_determined_and_follows_it() {
    let mut s = SpatialSystem::new();
    let a = pt(&mut s, [0.0, 0.0, 5.0]);
    let b = pt(&mut s, [9.0, 0.5, 5.0]);
    let held = s
        .add_constraint(Fix {
            point: a,
            at: [0.0, 0.0, 5.0],
        })
        .unwrap();
    let line = s.add_line(a, b).unwrap();
    s.add_constraint(Align {
        line,
        axis: Axis::X,
        sense: Positive,
    })
    .unwrap();
    let on = s
        .add_constraint(OnSurface {
            point: b,
            surface: SpatialSurface::Cylinder {
                origin: [0.0; 3],
                axis: Z,
                radius: 10.0,
            },
        })
        .unwrap();
    solve_ok(&mut s);
    assert!(near(s.point(b).unwrap(), [10.0, 0.0, 5.0]));
    let analysis = s.analyze();
    assert_eq!(analysis.dof, 0);
    assert!(analysis.is_fully_constrained());
    assert!(analysis.is_point_fully_constrained(b));
    s.set_surface(
        on,
        SpatialSurface::Cylinder {
            origin: [0.0; 3],
            axis: Z,
            radius: 12.5,
        },
    )
    .unwrap();
    solve_ok(&mut s);
    assert!(near(s.point(b).unwrap(), [12.5, 0.0, 5.0]));
    // Another kind of relation is no surface's.
    let sphere = SpatialSurface::Sphere {
        center: [0.0; 3],
        radius: 1.0,
    };
    assert!(matches!(
        s.set_surface(held, sphere),
        Err(SpatialError::WrongConstraintKind { .. })
    ));
}

/// A point at fixed offsets in X and Y from the origin on a sphere: its
/// height is determined, on the branch it starts on.
#[test]
fn point_on_a_sphere_by_two_offsets() {
    let mut s = SpatialSystem::new();
    let o = pt(&mut s, [0.0; 3]);
    fix(&mut s, o);
    let p = pt(&mut s, [3.0, 4.0, 10.0]);
    for (axis, value) in [(Axis::X, 6.0), (Axis::Y, 8.0)] {
        s.add_constraint(Offset {
            a: o,
            b: p,
            axis,
            value,
        })
        .unwrap();
    }
    s.add_constraint(OnSurface {
        point: p,
        surface: SpatialSurface::Sphere {
            center: [0.0; 3],
            radius: 26.0,
        },
    })
    .unwrap();
    solve_ok(&mut s);
    assert!(near(s.point(p).unwrap(), [6.0, 8.0, 24.0]));
    assert_eq!(s.analyze().dof, 0);
}

/// Points on a cone and on a torus, each at a given height and angle.
#[test]
fn points_on_a_cone_and_a_torus() {
    let mut s = SpatialSystem::new();
    let o = pt(&mut s, [0.0; 3]);
    fix(&mut s, o);
    // The cone: radius 10 at z = 0, 45 degrees, so 15 at z = 5.
    let c = pt(&mut s, [14.0, 0.0, 5.0]);
    let t = pt(&mut s, [26.0, 0.0, 4.0]);
    for (p, z) in [(c, 5.0), (t, 3.0)] {
        s.add_constraint(Offset {
            a: o,
            b: p,
            axis: Axis::Z,
            value: z,
        })
        .unwrap();
        s.add_constraint(Offset {
            a: o,
            b: p,
            axis: Axis::Y,
            value: 0.0,
        })
        .unwrap();
    }
    s.add_constraint(OnSurface {
        point: c,
        surface: SpatialSurface::Cone {
            origin: [0.0; 3],
            axis: Z,
            radius: 10.0,
            half_angle: FRAC_PI_4,
        },
    })
    .unwrap();
    // The torus: the tube of radius 5 about the circle of radius 20; at
    // z = 3 its outer side is 4 off the circle.
    s.add_constraint(OnSurface {
        point: t,
        surface: SpatialSurface::Torus {
            center: [0.0; 3],
            axis: Z,
            major: 20.0,
            minor: 5.0,
        },
    })
    .unwrap();
    solve_ok(&mut s);
    assert!(near(s.point(c).unwrap(), [15.0, 0.0, 5.0]));
    assert!(near(s.point(t).unwrap(), [24.0, 0.0, 3.0]));
    assert_eq!(s.analyze().dof, 0);
}

/// A point on the sketch plane and on a face in that plane: the second
/// relation takes no freedom (redundant, consistent); a point fixed off a
/// face and put on it contradicts its Fix; an invalid surface is refused.
#[test]
fn redundant_and_contradictory_surfaces() {
    let mut s = SpatialSystem::new();
    let p = pt(&mut s, [1.0, 2.0, 0.0]);
    s.add_constraint(OnSketchPlane(p)).unwrap();
    let face = s
        .add_constraint(OnSurface {
            point: p,
            surface: SpatialSurface::Plane {
                origin: [5.0, 5.0, 0.0],
                normal: Z,
            },
        })
        .unwrap();
    solve_ok(&mut s);
    let a = s.analyze();
    assert_eq!((a.rank, a.dof), (1, 2));
    assert_eq!(a.redundant_constraints(), vec![face]);

    let mut s = SpatialSystem::new();
    let q = pt(&mut s, [0.0, 0.0, 1.0]);
    let held = s
        .add_constraint(Fix {
            point: q,
            at: [0.0, 0.0, 1.0],
        })
        .unwrap();
    let on = s
        .add_constraint(OnSurface {
            point: q,
            surface: SpatialSurface::Plane {
                origin: [0.0; 3],
                normal: Z,
            },
        })
        .unwrap();
    let r = s.solve(&SpatialOptions::default());
    assert_eq!(r.status, SpatialStatus::Conflicting, "{r:?}");
    assert_eq!(r.conflicting(), vec![held, on]);
    assert!(near(s.point(q).unwrap(), [0.0, 0.0, 1.0]), "nothing moved");

    let bad = s.add_constraint(OnSurface {
        point: q,
        surface: SpatialSurface::Sphere {
            center: [0.0; 3],
            radius: 0.0,
        },
    });
    assert!(matches!(bad, Err(SpatialError::InvalidValue(_))));
}
