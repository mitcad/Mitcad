// SPDX-License-Identifier: MIT
//! A spline joined to an elliptical arc's end (mitcad#2107, Blend Curve): a
//! tangent holds the spline's leg along the arc's tangent there (G1), and a
//! smooth constraint its curvature too (G2), both to 1e-6.

mod common;

use common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::System;

/// The curvature of a Bézier span of degree `n` at its first control
/// point, run from there into the span: `(n - 1) / n * (d1 x d2) / |d1|^3`
/// with `d1` and `d2` its first two legs.
fn bezier_start_curvature(control: &[[f64; 2]]) -> f64 {
    let n = (control.len() - 1) as f64;
    let d1 = [control[1][0] - control[0][0], control[1][1] - control[0][1]];
    let d2 = [control[2][0] - control[1][0], control[2][1] - control[1][1]];
    let len = d1[0].hypot(d1[1]);
    (n - 1.0) / n * (d1[0] * d2[1] - d1[1] * d2[0]) / (len * len * len)
}

#[test]
fn a_spline_is_tangent_to_an_elliptical_arc_at_its_start() {
    let mut s = System::new();
    // The quarter of the ellipse about the origin, semi-axes 10 along x and
    // 5, from (10, 0) to (0, 5): at (10, 0) its tangent is vertical.
    let c = s.add_point(0.0, 0.0);
    let major = s.add_point(10.0, 0.0);
    let start = s.add_point(10.0, 0.0);
    let end = s.add_point(0.0, 5.0);
    let arc = s.add_elliptical_arc(c, major, 5.0, start, end).unwrap();
    s.add_constraint(FixEntity(arc)).unwrap();
    // A cubic ending at the arc's start, its last leg off the vertical.
    let ctrl = [
        s.add_point(4.0, -12.0),
        s.add_point(6.0, -9.0),
        s.add_point(11.0, -4.0),
        start,
    ];
    let sp = s.add_bspline(3, &ctrl, None, None).unwrap();
    s.add_constraint(FixPoint(ctrl[0])).unwrap();
    s.add_constraint(TangentAt {
        a: sp,
        b: arc,
        at: start,
    })
    .unwrap();
    solve_ok(&mut s);
    let g = s.spline(sp).unwrap();
    let n = g.control_points.len() - 1;
    let (p, q) = (g.control_points[n - 1], g.control_points[n]);
    assert!(
        (p[0] - q[0]).abs() < 1e-6,
        "G1: the last leg is vertical, {p:?} {q:?}"
    );
    assert!(dist(q, [10.0, 0.0]) < 1e-9, "the arc stays fixed");
    let an = s.analyze();
    assert!(an.redundant.is_empty() && an.conflicts.is_empty(), "{an:?}");
}

#[test]
fn a_smooth_spline_continues_an_elliptical_arc_with_its_curvature() {
    let mut s = System::new();
    // The same quarter: at its end (0, 5) the arc runs on along -x, its
    // curvature there |A| b / |E'|^3 = 10 * 5 / 10^3 = 0.05, turning left.
    let c = s.add_point(0.0, 0.0);
    let major = s.add_point(10.0, 0.0);
    let start = s.add_point(10.0, 0.0);
    let end = s.add_point(0.0, 5.0);
    let arc = s.add_elliptical_arc(c, major, 5.0, start, end).unwrap();
    s.add_constraint(FixEntity(arc)).unwrap();
    // A quintic (one Bézier span) from the arc's end.
    let ctrl = [
        end,
        s.add_point(-3.0, 5.6),
        s.add_point(-6.0, 4.5),
        s.add_point(-9.0, 6.0),
        s.add_point(-12.0, 3.0),
        s.add_point(-15.0, 1.0),
    ];
    let sp = s.add_bspline(5, &ctrl, None, None).unwrap();
    s.add_constraint(FixPoint(ctrl[5])).unwrap();
    s.add_constraint(SmoothAt {
        a: sp,
        b: arc,
        at: end,
    })
    .unwrap();
    solve_ok(&mut s);
    let g = s.spline(sp).unwrap();
    let cp = &g.control_points;
    assert!(dist(cp[0], [0.0, 5.0]) < 1e-9, "the arc stays fixed");
    assert!(
        (cp[1][1] - 5.0).abs() < 1e-6 && cp[1][0] < 0.0,
        "G1: the first leg runs on along -x, {cp:?}"
    );
    let k = bezier_start_curvature(cp);
    assert!((k - 0.05).abs() < 1e-6, "G2: curvature at the join {k}");
    let an = s.analyze();
    assert!(an.redundant.is_empty() && an.conflicts.is_empty(), "{an:?}");
}

#[test]
fn an_elliptical_arc_is_tangent_only_to_a_spline_at_its_ends() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let major = s.add_point(10.0, 0.0);
    let start = s.add_point(10.0, 0.0);
    let end = s.add_point(0.0, 5.0);
    let arc = s.add_elliptical_arc(c, major, 5.0, start, end).unwrap();
    // A line, and a spline that does not end at the arc.
    let a = s.add_point(10.0, -5.0);
    let line = s.add_line(a, start).unwrap();
    assert!(s.add_constraint(Tangent(line, arc)).is_err());
    let ctrl = [
        s.add_point(20.0, 0.0),
        s.add_point(22.0, 3.0),
        s.add_point(25.0, 1.0),
        s.add_point(28.0, 4.0),
    ];
    let sp = s.add_bspline(3, &ctrl, None, None).unwrap();
    assert!(s.add_constraint(Tangent(sp, arc)).is_err());
    assert!(s.add_constraint(Smooth(sp, arc)).is_err());
}
