// SPDX-License-Identifier: MIT
//! Perpendicular with circles and arcs (mitcad#2114): a line and a circle
//! or arc hold the center on the infinite line; two circles or arcs cross
//! at right angles, `|c1 c2|^2 = r1^2 + r2^2`; a common center conflicts.

mod common;

use common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::{DragGoal, Error, SolveStatus, System};

/// The distance of `p` from the infinite line through `a` and `b`.
fn off_line(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / dx.hypot(dy)
}

#[test]
fn a_line_and_an_arc_hold_the_center_on_the_line() {
    let mut s = System::new();
    let (a, b) = (s.add_point(0.0, 0.0), s.add_point(10.0, 1.0));
    let line = s.add_line(a, b).unwrap();
    let c = s.add_point(20.0, 6.0);
    let (st, en) = (s.add_point(25.0, 6.0), s.add_point(20.0, 11.0));
    let arc = s.add_arc(c, st, en).unwrap();
    let free = s.analyze().dof;
    // The arc first: either order is taken.
    s.add_constraint(Perpendicular(arc, line)).unwrap();
    solve_ok(&mut s);
    assert!(off_line(pt(&s, c), pt(&s, a), pt(&s, b)) < EPS);
    assert_eq!(s.analyze().dof, free - 1);

    // Dragging the center takes the line with it.
    s.add_constraint(FixPoint(a)).unwrap();
    let r = s.drag(
        &[DragGoal::Point {
            point: c,
            target: [5.0, 15.0],
        }],
        &opts(),
    );
    assert!(r.is_ok(), "{r:?}");
    assert!(dist(pt(&s, c), [5.0, 15.0]) < 1e-3, "{:?}", pt(&s, c));
    assert!(off_line(pt(&s, c), pt(&s, a), pt(&s, b)) < EPS);
}

#[test]
fn two_arcs_cross_at_right_angles() {
    let mut s = System::new();
    let c1 = s.add_point(0.0, 0.0);
    let (s1, e1) = (s.add_point(10.0, 0.0), s.add_point(0.0, 10.0));
    let a1 = s.add_arc(c1, s1, e1).unwrap();
    let c2 = s.add_point(15.0, 3.0);
    let (s2, e2) = (s.add_point(15.0, -3.0), s.add_point(15.0, 9.0));
    // Counter-clockwise from the bottom through the left.
    let a2 = s.add_arc(c2, e2, s2).unwrap();
    s.add_constraint(FixPoint(c1)).unwrap();
    let free = s.analyze().dof;
    s.add_constraint(Perpendicular(a1, a2)).unwrap();
    let radius = s
        .add_constraint(Radius {
            entity: a1,
            value: 10.0,
        })
        .unwrap();
    s.add_constraint(Radius {
        entity: a2,
        value: 6.0,
    })
    .unwrap();
    let check = |s: &System, r1: f64, r2: f64| {
        let d = dist(pt(s, c1), pt(s, c2));
        let gap = d * d - r1 * r1 - r2 * r2;
        assert!(gap.abs() < 1e-6, "|c1 c2|^2 - r1^2 - r2^2 = {gap}");
        // At a crossing the radii, and so the tangents, are square.
        let (k1, k2) = (pt(s, c1), pt(s, c2));
        let along = [(k2[0] - k1[0]) / d, (k2[1] - k1[1]) / d];
        let x = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
        let h = (r1 * r1 - x * x).sqrt();
        let p = [
            k1[0] + along[0] * x - along[1] * h,
            k1[1] + along[1] * x + along[0] * h,
        ];
        let (u, v) = ([p[0] - k1[0], p[1] - k1[1]], [p[0] - k2[0], p[1] - k2[1]]);
        let angle = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
        assert!(
            (angle.abs() - std::f64::consts::FRAC_PI_2).abs() <= 1e-6,
            "the tangents meet at {angle} rad"
        );
    };
    solve_ok(&mut s);
    check(&s, 10.0, 6.0);
    assert_eq!(s.analyze().dof, free - 3);

    s.set_dimension_value(radius, 14.0).unwrap();
    solve_ok(&mut s);
    assert_near(dist(pt(&s, c1), pt(&s, s1)), 14.0, "first radius");
    check(&s, 14.0, 6.0);
}

#[test]
fn perpendicular_circles_with_a_common_center_conflict() {
    let mut s = System::new();
    let (c1, c2) = (s.add_point(0.0, 0.0), s.add_point(3.0, 1.0));
    let a = s.add_circle(c1, 5.0).unwrap();
    let b = s.add_circle(c2, 4.0).unwrap();
    let perpendicular = s.add_constraint(Perpendicular(a, b)).unwrap();
    solve_ok(&mut s);
    let concentric = s.add_constraint(Concentric(b, a)).unwrap();
    let before = (s.radius(a).unwrap(), s.radius(b).unwrap());
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert_eq!(r.conflicting(), vec![perpendicular, concentric]);
    // Left where it was, not solved to a point.
    assert_eq!((s.radius(a).unwrap(), s.radius(b).unwrap()), before);
    let told = s.analyze().conflicting_constraints();
    assert!(
        told.contains(&perpendicular) && told.contains(&concentric),
        "{told:?}"
    );
}

#[test]
fn perpendicular_takes_no_ellipse() {
    let mut s = System::new();
    let (a, b) = (s.add_point(0.0, 0.0), s.add_point(10.0, 0.0));
    let line = s.add_line(a, b).unwrap();
    let (c, major) = (s.add_point(5.0, 5.0), s.add_point(9.0, 5.0));
    let ellipse = s.add_ellipse(c, major, 2.0).unwrap();
    assert!(matches!(
        s.add_constraint(Perpendicular(line, ellipse)),
        Err(Error::WrongEntityKind { .. })
    ));
}
