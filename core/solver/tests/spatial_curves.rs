// SPDX-License-Identifier: MIT
//! Arcs, circles, splines, curve relations and in-plane relations of the
//! spatial system (mitcad#1542): the solver study's analytical fixtures
//! 9-15 as executable cases (statuses, ranks and coordinates at 1e-6 mm),
//! with refused sense flips, degenerate input, a parameter at its bound and
//! a planar comparison of the in-plane rows.

use std::f64::consts::PI;

use mitcad_solver::AxisSense::{Negative, Positive};
use mitcad_solver::SpatialConstraint::*;
use mitcad_solver::{
    Axis, Constraint, SolveOptions, SpatialCurve, SpatialDragGoal, SpatialError, SpatialIssue,
    SpatialNormal, SpatialOptions, SpatialPointId, SpatialReferenceCurve, SpatialSolveResult,
    SpatialStatus, SpatialSystem, SpatialTangentGoal, SplineEnd, System, circle_through,
};

/// Geometric comparison tolerance (mm).
const EPS: f64 = 1e-6;
/// Where a soft drag goal that the relations do not let it reach ends
/// (mm): at the nearest place they allow, within the drag's own stopping
/// rule.
const DRAG_EPS: f64 = 1e-4;

fn opts() -> SpatialOptions {
    SpatialOptions::default()
}

fn solve_ok(s: &mut SpatialSystem) -> SpatialSolveResult {
    let r = s.solve(&opts());
    assert!(r.is_ok(), "solve failed: {r:?}");
    r
}

fn near(v: [f64; 3], want: [f64; 3], eps: f64) -> bool {
    (0..3).all(|k| (v[k] - want[k]).abs() < eps)
}

fn assert_at(s: &SpatialSystem, p: SpatialPointId, want: [f64; 3]) {
    let v = s.point(p).expect("point");
    assert!(
        near(v, want, EPS),
        "point {p:?} at {v:?}, expected {want:?}"
    );
}

fn fix(s: &mut SpatialSystem, p: SpatialPointId) {
    let at = s.point(p).expect("point");
    s.add_constraint(Fix { point: p, at }).unwrap();
}

fn pt(s: &mut SpatialSystem, v: [f64; 3]) -> SpatialPointId {
    s.add_point(v).unwrap()
}

/// Fixture 9: a free arc has 12 unknowns and 4 internal rows (DOF 8); with
/// both ends fixed DOF 2 (radius and rotation about the chord); a
/// plane-bound arc has DOF 6.
#[test]
fn arc_degrees_of_freedom() {
    let mut s = SpatialSystem::new();
    let c = pt(&mut s, [0.0, 0.0, 0.0]);
    let st = pt(&mut s, [10.0, 0.0, 0.0]);
    let e = pt(&mut s, [0.0, 10.0, 0.0]);
    let arc = s
        .add_arc(c, st, e, SpatialNormal::Free([0.0, 0.0, 2.0]))
        .unwrap();
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (12, 4, 8));
    assert!(a.redundant.is_empty() && a.conflicts.is_empty() && a.issues.is_empty());
    assert!((s.radius(arc).unwrap() - 10.0).abs() < EPS);
    assert!((s.arc_length(arc).unwrap() - 5.0 * PI).abs() < EPS);
    let n = s.normal(arc).unwrap();
    assert!(near(n, [0.0, 0.0, 1.0], EPS));
    fix(&mut s, st);
    fix(&mut s, e);
    solve_ok(&mut s);
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (12, 10, 2));
    assert!(a.redundant.is_empty() && a.conflicts.is_empty() && a.issues.is_empty());
    assert!(!a.is_curve_fully_constrained(arc));

    let mut s = SpatialSystem::new();
    let c = pt(&mut s, [0.0, 0.0, 3.0]);
    let st = pt(&mut s, [10.0, 0.0, 3.0]);
    let e = pt(&mut s, [0.0, 10.0, 3.0]);
    s.add_arc(c, st, e, SpatialNormal::Axis(Axis::Z, Positive))
        .unwrap();
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (9, 3, 6));
}

/// Fixture 10: the half circle. With its centre and start fixed and its
/// end held level with the start, the end goes to the far side and the
/// normal keeps one freedom (rotation about the diameter): DOF 1, and the
/// dependency among the arc's own plane rows is its geometry, no relation's
/// redundancy. Aligning the normal with Z leaves DOF 0.
#[test]
fn half_circle_turns_about_its_diameter() {
    let mut s = SpatialSystem::new();
    let c = pt(&mut s, [0.0, 0.0, 0.0]);
    let st = pt(&mut s, [-10.0, 0.0, 0.0]);
    let e = pt(&mut s, [9.0, 0.5, -0.3]);
    let arc = s
        .add_arc(c, st, e, SpatialNormal::Free([0.1, 0.2, 1.0]))
        .unwrap();
    fix(&mut s, c);
    fix(&mut s, st);
    for axis in [Axis::Y, Axis::Z] {
        s.add_constraint(Offset {
            a: st,
            b: e,
            axis,
            value: 0.0,
        })
        .unwrap();
    }
    solve_ok(&mut s);
    assert_at(&s, e, [10.0, 0.0, 0.0]);
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (12, 11, 1));
    assert!(a.redundant.is_empty(), "{:?}", a.redundant);
    assert!(a.conflicts.is_empty() && a.issues.is_empty());
    assert!(a.solved);
    let n = s.normal(arc).unwrap();
    assert!(n[0].abs() < EPS);

    s.add_constraint(NormalAlign {
        curve: arc,
        axis: Axis::Z,
        sense: Positive,
    })
    .unwrap();
    solve_ok(&mut s);
    assert!(near(s.normal(arc).unwrap(), [0.0, 0.0, 1.0], EPS));
    let a = s.analyze();
    assert_eq!(a.dof, 0);
    assert!(a.conflicts.is_empty() && a.issues.is_empty());
    assert!(a.is_curve_fully_constrained(arc));
}

/// Fixture 11: the bent route W11. Lines AB, CD, EF along +X, +Y, +Z
/// joined by two tangent arcs of equal radius: 30 unknowns, rank 30. Then
/// AB 60 and radius 12.
#[test]
fn bent_route_w11() {
    let mut s = SpatialSystem::new();
    let a = pt(&mut s, [0.2, -0.1, 0.1]);
    let b = pt(&mut s, [39.0, 0.5, -0.3]);
    let c1 = pt(&mut s, [40.5, 9.5, 0.2]);
    let c = pt(&mut s, [49.5, 10.3, 0.1]);
    let d = pt(&mut s, [50.2, 39.5, -0.2]);
    let c2 = pt(&mut s, [49.8, 40.3, 10.2]);
    let e = pt(&mut s, [50.1, 49.7, 9.6]);
    let f = pt(&mut s, [49.9, 50.2, 29.5]);
    let ab = s.add_line(a, b).unwrap();
    let cd = s.add_line(c, d).unwrap();
    let ef = s.add_line(e, f).unwrap();
    let arc1 = s
        .add_arc(c1, b, c, SpatialNormal::Free([0.05, -0.02, 1.0]))
        .unwrap();
    let arc2 = s
        .add_arc(c2, d, e, SpatialNormal::Free([1.0, 0.03, -0.04]))
        .unwrap();
    s.add_constraint(Attach {
        point: a,
        target: [0.0; 3],
    })
    .unwrap();
    let mut length = Vec::new();
    for (line, axis, value) in [
        (ab, Axis::X, 40.0),
        (cd, Axis::Y, 30.0),
        (ef, Axis::Z, 20.0),
    ] {
        s.add_constraint(Align {
            line,
            axis,
            sense: Positive,
        })
        .unwrap();
        length.push(s.add_constraint(Length { line, value }).unwrap());
    }
    let radius = s
        .add_constraint(Radius {
            curve: arc1,
            value: 10.0,
        })
        .unwrap();
    s.add_constraint(Equal(SpatialCurve::Curve(arc1), SpatialCurve::Curve(arc2)))
        .unwrap();
    for (x, y, at) in [
        (SpatialCurve::Line(ab), SpatialCurve::Curve(arc1), b),
        (SpatialCurve::Curve(arc1), SpatialCurve::Line(cd), c),
        (SpatialCurve::Line(cd), SpatialCurve::Curve(arc2), d),
        (SpatialCurve::Curve(arc2), SpatialCurve::Line(ef), e),
    ] {
        s.add_constraint(Tangent { a: x, b: y, at }).unwrap();
    }
    solve_ok(&mut s);
    for (p, want) in [
        (a, [0.0, 0.0, 0.0]),
        (b, [40.0, 0.0, 0.0]),
        (c1, [40.0, 10.0, 0.0]),
        (c, [50.0, 10.0, 0.0]),
        (d, [50.0, 40.0, 0.0]),
        (c2, [50.0, 40.0, 10.0]),
        (e, [50.0, 50.0, 10.0]),
        (f, [50.0, 50.0, 30.0]),
    ] {
        assert_at(&s, p, want);
    }
    assert!(near(s.normal(arc1).unwrap(), [0.0, 0.0, 1.0], EPS));
    assert!(near(s.normal(arc2).unwrap(), [1.0, 0.0, 0.0], EPS));
    let an = s.analyze();
    assert_eq!((an.unknowns, an.rank, an.dof), (30, 30, 0));
    assert!(an.redundant.is_empty() && an.conflicts.is_empty() && an.issues.is_empty());
    assert!(an.is_fully_constrained());
    assert!(an.is_curve_fully_constrained(arc1) && an.is_curve_fully_constrained(arc2));

    s.set_value(length[0], 60.0).unwrap();
    s.set_value(radius, 12.0).unwrap();
    solve_ok(&mut s);
    for (p, want) in [
        (b, [60.0, 0.0, 0.0]),
        (c1, [60.0, 12.0, 0.0]),
        (c, [72.0, 12.0, 0.0]),
        (d, [72.0, 42.0, 0.0]),
        (c2, [72.0, 42.0, 12.0]),
        (e, [72.0, 54.0, 12.0]),
        (f, [72.0, 54.0, 32.0]),
    ] {
        assert_at(&s, p, want);
    }
    assert!((s.measure(radius).unwrap() - 12.0).abs() < EPS);
    assert!((s.arc_length(arc2).unwrap() - 6.0 * PI).abs() < EPS);
}

/// Fixture 12: four free fit points with natural ends: 12 + 12 hidden
/// unknowns, 6 interpolation and 6 end rows, DOF 12. One end tangent to a
/// fixed line: its 3 end rows go and 2 tangency rows come, DOF 13 (the
/// extra one the end tangent's magnitude).
#[test]
fn fit_point_spline_and_a_tangent_end() {
    let mut s = SpatialSystem::new();
    let fit: Vec<SpatialPointId> = [
        [0.0, 0.0, 0.0],
        [10.0, 5.0, 2.0],
        [20.0, 0.0, 4.0],
        [30.0, 6.0, 1.0],
    ]
    .iter()
    .map(|&v| pt(&mut s, v))
    .collect();
    let spline = s.add_fitted_spline(&fit, [SplineEnd::Natural; 2]).unwrap();
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (24, 12, 12));
    assert!(a.solved && a.issues.is_empty());
    let geometry = s.spline(spline).unwrap();
    assert_eq!(geometry.control_points.len(), 6);
    assert!(near(geometry.control_points[0], [0.0; 3], EPS));
    assert!(s.spline_length(spline).unwrap() > 30.0);

    let l0 = pt(&mut s, [-10.0, -3.0, 0.0]);
    let l1 = pt(&mut s, [-5.0, -2.0, 0.5]);
    let line = s.add_line(l0, l1).unwrap();
    fix(&mut s, l0);
    fix(&mut s, l1);
    s.add_constraint(Tangent {
        a: SpatialCurve::Line(line),
        b: SpatialCurve::Curve(spline),
        at: fit[0],
    })
    .unwrap();
    solve_ok(&mut s);
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (30, 17, 13));
    assert!(a.redundant.is_empty() && a.conflicts.is_empty() && a.issues.is_empty());
    // The start tangent along the line.
    let g = s.spline(spline).unwrap();
    let d = [0, 1, 2].map(|k| g.control_points[1][k] - g.control_points[0][k]);
    let u = [5.0, 1.0, 0.5];
    let cross = [
        d[1] * u[2] - d[2] * u[1],
        d[2] * u[0] - d[0] * u[2],
        d[0] * u[1] - d[1] * u[0],
    ];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    assert!(cross.iter().all(|x| (x / len).abs() < 1e-6));
}

/// A free end's tangent `C'(t)` is a soft drag goal; the fit points stay.
#[test]
fn dragging_a_free_end_tangent() {
    let mut s = SpatialSystem::new();
    let fit: Vec<SpatialPointId> = [
        [0.0, 0.0, 0.0],
        [10.0, 5.0, 2.0],
        [20.0, 0.0, 4.0],
        [30.0, 6.0, 1.0],
    ]
    .iter()
    .map(|&v| pt(&mut s, v))
    .collect();
    let spline = s
        .add_fitted_spline(&fit, [SplineEnd::Natural, SplineEnd::Free])
        .unwrap();
    for &p in &fit {
        fix(&mut s, p);
    }
    let a = s.analyze();
    assert_eq!(a.dof, 3);
    let target = [40.0, -10.0, 5.0];
    let r = s
        .drag_with(
            &[],
            &[SpatialTangentGoal {
                spline,
                end: 1,
                target,
            }],
            &opts(),
        )
        .unwrap();
    assert!(r.is_ok(), "{r:?}");
    let g = s.spline(spline).unwrap();
    let n = g.control_points.len() - 1;
    let k = g.knots[n + g.degree] - g.knots[n];
    let d = [0, 1, 2]
        .map(|j| g.degree as f64 * (g.control_points[n][j] - g.control_points[n - 1][j]) / k);
    assert!(near(d, target, 1e-3), "{d:?}");
    assert_at(&s, fit[3], [30.0, 6.0, 1.0]);
    // Not a closed spline or a missing end.
    assert!(matches!(
        s.drag_with(
            &[],
            &[SpatialTangentGoal {
                spline,
                end: 2,
                target,
            }],
            &opts(),
        ),
        Err(SpatialError::InvalidValue(_))
    ));
}

/// Fixture 13: a point on a constant vertical reference edge (4 unknowns,
/// 3 rows, DOF 1), held 15 mm above the attached bottom vertex; the edge
/// grows to 60 and the point stays. An edge midpoint follows the source.
#[test]
fn points_on_and_at_the_middle_of_a_reference_edge() {
    let mut s = SpatialSystem::new();
    let edge = s
        .add_reference_curve(SpatialReferenceCurve::Line {
            start: [0.0; 3],
            end: [0.0, 0.0, 40.0],
        })
        .unwrap();
    let p = pt(&mut s, [0.3, -0.2, 14.0]);
    s.add_constraint(PointOn {
        point: p,
        curve: SpatialCurve::Curve(edge),
    })
    .unwrap();
    solve_ok(&mut s);
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (4, 3, 1));
    let q = s.point(p).unwrap();
    assert!(q[0].abs() < EPS && q[1].abs() < EPS);

    let v = pt(&mut s, [0.0; 3]);
    s.add_constraint(Attach {
        point: v,
        target: [0.0; 3],
    })
    .unwrap();
    s.add_constraint(Offset {
        a: v,
        b: p,
        axis: Axis::Z,
        value: 15.0,
    })
    .unwrap();
    let m = pt(&mut s, [0.5, 0.5, 18.0]);
    s.add_constraint(Midpoint {
        point: m,
        curve: SpatialCurve::Curve(edge),
    })
    .unwrap();
    solve_ok(&mut s);
    assert_at(&s, p, [0.0, 0.0, 15.0]);
    assert_at(&s, m, [0.0, 0.0, 20.0]);
    assert_eq!(s.analyze().dof, 0);

    s.set_reference_curve(
        edge,
        SpatialReferenceCurve::Line {
            start: [0.0; 3],
            end: [0.0, 0.0, 60.0],
        },
    )
    .unwrap();
    solve_ok(&mut s);
    assert_at(&s, p, [0.0, 0.0, 15.0]);
    assert_at(&s, m, [0.0, 0.0, 30.0]);
    assert!((s.spline_length(edge).unwrap() - 60.0).abs() < 1e-9);
}

/// A curve parameter at its bound is a valid solution (a point at the end
/// of a line); beyond it there is none.
#[test]
fn a_point_on_a_line_at_its_end() {
    let mut s = SpatialSystem::new();
    let a = pt(&mut s, [0.0; 3]);
    let b = pt(&mut s, [10.0, 0.0, 0.0]);
    let l = s.add_line(a, b).unwrap();
    fix(&mut s, a);
    fix(&mut s, b);
    let p = pt(&mut s, [6.0, 0.5, -0.5]);
    s.add_constraint(PointOn {
        point: p,
        curve: SpatialCurve::Line(l),
    })
    .unwrap();
    let off = s
        .add_constraint(Offset {
            a,
            b: p,
            axis: Axis::X,
            value: 10.0,
        })
        .unwrap();
    solve_ok(&mut s);
    assert_at(&s, p, [10.0, 0.0, 0.0]);
    let a2 = s.analyze();
    assert_eq!(a2.dof, 0);
    assert!(a2.conflicts.is_empty() && a2.issues.is_empty());
    s.set_value(off, 12.0).unwrap();
    let r = s.solve(&opts());
    assert!(!r.is_ok(), "{r:?}");
    assert_at(&s, p, [10.0, 0.0, 0.0]);
    // A point that defines the curve is not on it.
    assert!(matches!(
        s.add_constraint(PointOn {
            point: a,
            curve: SpatialCurve::Line(l),
        }),
        Err(SpatialError::InvalidValue(_))
    ));
}

/// A point on a control-point spline slides along it within its range.
#[test]
fn a_point_on_a_spline() {
    let mut s = SpatialSystem::new();
    let ctrl: Vec<SpatialPointId> = [
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [20.0, 0.0, 0.0],
        [30.0, 0.0, 0.0],
    ]
    .iter()
    .map(|&v| pt(&mut s, v))
    .collect();
    for &p in &ctrl {
        fix(&mut s, p);
    }
    let spline = s.add_bspline(3, &ctrl, None, None).unwrap();
    let p = pt(&mut s, [12.0, 1.0, -1.0]);
    s.add_constraint(PointOn {
        point: p,
        curve: SpatialCurve::Curve(spline),
    })
    .unwrap();
    solve_ok(&mut s);
    let q = s.point(p).unwrap();
    assert!(q[1].abs() < EPS && q[2].abs() < EPS && (0.0..=30.0).contains(&q[0]));
    let a = s.analyze();
    assert_eq!(a.dof, 1);
    assert!(a.issues.is_empty());
}

/// Arc midpoints: on the arc, and refused on its complement.
#[test]
fn arc_midpoint_and_its_side() {
    let build = |start: [f64; 3]| {
        let mut s = SpatialSystem::new();
        let c = pt(&mut s, [0.0; 3]);
        let st = pt(&mut s, [10.0, 0.0, 0.0]);
        let e = pt(&mut s, [0.0, 10.0, 0.0]);
        let arc = s
            .add_arc(c, st, e, SpatialNormal::Axis(Axis::Z, Positive))
            .unwrap();
        for p in [c, st, e] {
            fix(&mut s, p);
        }
        let m = pt(&mut s, start);
        let id = s
            .add_constraint(Midpoint {
                point: m,
                curve: SpatialCurve::Curve(arc),
            })
            .unwrap();
        (s, m, id)
    };
    let (mut s, m, _) = build([6.0, 8.0, 0.5]);
    solve_ok(&mut s);
    let h = 10.0 / 2f64.sqrt();
    assert_at(&s, m, [h, h, 0.0]);
    assert_eq!(s.analyze().dof, 0);

    let (mut s, m, id) = build([-6.0, -8.0, 0.5]);
    let r = s.solve(&opts());
    assert_eq!(r.status, SpatialStatus::Invalid);
    assert!(r.issues.contains(&SpatialIssue::MidpointOffArc(id)));
    assert_at(&s, m, [-6.0, -8.0, 0.5]);
}

/// Radius, diameter, arc length and equal radii; dragging an arc's end.
#[test]
fn arc_dimensions_and_dragging_an_arc_point() {
    let mut s = SpatialSystem::new();
    let c = pt(&mut s, [0.0; 3]);
    let st = pt(&mut s, [10.0, 0.0, 0.0]);
    let e = pt(&mut s, [1.0, 9.0, 0.5]);
    let arc = s
        .add_arc(c, st, e, SpatialNormal::Free([0.0, 0.1, 1.0]))
        .unwrap();
    fix(&mut s, c);
    fix(&mut s, st);
    s.add_constraint(NormalAlign {
        curve: arc,
        axis: Axis::Z,
        sense: Positive,
    })
    .unwrap();
    let a = s.analyze();
    assert_eq!(a.dof, 1);
    // A drag moves the end along the circle toward the target.
    let r = s
        .drag(
            &[SpatialDragGoal {
                point: e,
                target: [0.0, 20.0, 0.0],
            }],
            &opts(),
        )
        .unwrap();
    assert!(r.is_ok(), "{r:?}");
    assert!(near(s.point(e).unwrap(), [0.0, 10.0, 0.0], DRAG_EPS));
    let len = s
        .add_constraint(ArcLength {
            curve: arc,
            value: 5.0 * PI,
        })
        .unwrap();
    s.set_point(e, [1.0, 9.0, 0.5]).unwrap();
    solve_ok(&mut s);
    assert_at(&s, e, [0.0, 10.0, 0.0]);
    assert!(s.analyze().is_fully_constrained());
    s.set_value(len, 10.0 * PI).unwrap();
    solve_ok(&mut s);
    assert_at(&s, e, [-10.0, 0.0, 0.0]);

    // A circle of diameter 30 and an arc of equal radius.
    let k = pt(&mut s, [50.0, 0.0, 0.0]);
    let circle = s
        .add_circle(k, 12.0, SpatialNormal::Axis(Axis::X, Negative))
        .unwrap();
    s.add_constraint(Diameter {
        curve: circle,
        value: 30.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert!((s.radius(circle).unwrap() - 15.0).abs() < EPS);
    let r2 = s
        .add_constraint(Equal(SpatialCurve::Curve(circle), SpatialCurve::Curve(arc)))
        .unwrap();
    let res = s.solve(&opts());
    assert_eq!(res.status, SpatialStatus::Conflicting, "{res:?}");
    assert!(res.conflicting().contains(&r2));
    s.remove_constraint(r2).unwrap();
    // Radii and diameters must have extent; equal takes two of a kind.
    assert!(matches!(
        s.add_constraint(Radius {
            curve: circle,
            value: 1e-7,
        }),
        Err(SpatialError::InvalidValue(_))
    ));
    assert!(matches!(
        s.add_circle(k, 1e-7, SpatialNormal::Free([0.0, 0.0, 1.0])),
        Err(SpatialError::InvalidValue(_))
    ));
}

/// A tangency keeps the sense it was added with: a solution where the
/// curves point against each other is refused, and nothing moves.
#[test]
fn a_tangency_sense_flip_is_refused() {
    let mut s = SpatialSystem::new();
    let a = pt(&mut s, [0.0; 3]);
    let b = pt(&mut s, [10.0, 0.0, 0.0]);
    let c = pt(&mut s, [0.0, 5.0, 0.0]);
    let d = pt(&mut s, [10.0, 5.5, 0.3]);
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(c, d).unwrap();
    fix(&mut s, a);
    fix(&mut s, b);
    fix(&mut s, c);
    let t = s
        .add_constraint(Tangent {
            a: SpatialCurve::Line(l1),
            b: SpatialCurve::Line(l2),
            at: c,
        })
        .unwrap();
    s.add_constraint(Length {
        line: l2,
        value: 10.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_at(&s, d, [10.0, 5.0, 0.0]);
    assert_eq!(s.analyze().dof, 0);
    // The second line asked to point the other way (started there, so the
    // solve itself is easy): the tangency holds, its sense does not.
    s.set_point(d, [-9.0, 5.5, 0.3]).unwrap();
    s.add_constraint(Offset {
        a: c,
        b: d,
        axis: Axis::X,
        value: -10.0,
    })
    .unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SpatialStatus::Invalid, "{r:?}");
    assert!(r.issues.contains(&SpatialIssue::ReversedTangent(t)));
    assert_at(&s, d, [-9.0, 5.5, 0.3]);
}

/// A general tangency's auxiliary axis is chosen anew when its tangent
/// turns onto it: a quarter turn keeps the rows regular.
#[test]
fn a_general_tangency_follows_a_quarter_turn() {
    let mut s = SpatialSystem::new();
    let a = pt(&mut s, [0.0; 3]);
    let b = pt(&mut s, [0.0, 10.0, 0.0]);
    let c = pt(&mut s, [5.0, 0.0, 0.0]);
    let d = pt(&mut s, [5.0, 9.0, 0.5]);
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(c, d).unwrap();
    fix(&mut s, a);
    let fb = s
        .add_constraint(Fix {
            point: b,
            at: [0.0, 10.0, 0.0],
        })
        .unwrap();
    fix(&mut s, c);
    s.add_constraint(Tangent {
        a: SpatialCurve::Line(l1),
        b: SpatialCurve::Line(l2),
        at: c,
    })
    .unwrap();
    s.add_constraint(Length {
        line: l2,
        value: 10.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_at(&s, d, [5.0, 10.0, 0.0]);
    // The first line turns from +Y to +X.
    s.set_target(fb, [10.0, 0.0, 0.0]).unwrap();
    solve_ok(&mut s);
    assert_at(&s, d, [15.0, 0.0, 0.0]);
    let an = s.analyze();
    assert_eq!((an.unknowns, an.rank, an.dof), (12, 12, 0));
    assert!(an.conflicts.is_empty() && an.issues.is_empty());
}

/// Degenerate input and geometry: the three-point helper refuses
/// collinear points; an arc whose ends meet is invalid geometry.
#[test]
fn near_degenerate_radius_and_sweep() {
    let circle = circle_through([0.0, 0.0, 0.0], [10.0, 10.0, 0.0], [20.0, 0.0, 0.0]).unwrap();
    assert!(near(circle.center, [10.0, 0.0, 0.0], 1e-9));
    assert!((circle.radius - 10.0).abs() < 1e-9);
    assert!(near(circle.normal, [0.0, 0.0, -1.0], 1e-9));
    assert!(circle_through([0.0; 3], [5.0, 1e-7, 0.0], [10.0, 0.0, 0.0]).is_none());
    assert!(circle_through([0.0; 3], [1e-7, 0.0, 0.0], [10.0, 3.0, 0.0]).is_none());
    assert!(circle_through([0.0; 3], [5.0, 1e-3, 0.0], [10.0, 0.0, 0.0]).is_some());

    // The end forced onto the start: no sweep.
    let mut s = SpatialSystem::new();
    let c = pt(&mut s, [0.0; 3]);
    let st = pt(&mut s, [10.0, 0.0, 0.0]);
    let e = pt(&mut s, [9.9, 1.0, 0.0]);
    let arc = s
        .add_arc(c, st, e, SpatialNormal::Free([0.0, 0.0, 1.0]))
        .unwrap();
    fix(&mut s, c);
    fix(&mut s, st);
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        s.add_constraint(Offset {
            a: st,
            b: e,
            axis,
            value: 0.0,
        })
        .unwrap();
    }
    let r = s.solve(&opts());
    assert_eq!(r.status, SpatialStatus::Invalid, "{r:?}");
    assert!(r.issues.contains(&SpatialIssue::DegenerateCurve(arc)));
    assert_at(&s, e, [9.9, 1.0, 0.0]);
    // An arc joined to itself is refused before solving.
    let mut s = SpatialSystem::new();
    let c = pt(&mut s, [0.0; 3]);
    let st = pt(&mut s, [10.0, 0.0, 0.0]);
    let e = pt(&mut s, [0.0, 10.0, 0.0]);
    let arc = s
        .add_arc(c, st, e, SpatialNormal::Axis(Axis::Z, Positive))
        .unwrap();
    s.add_constraint(Coincident(st, e)).unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SpatialStatus::Invalid);
    assert!(r.issues.contains(&SpatialIssue::DegenerateCurve(arc)));
    assert!(matches!(
        s.add_arc(c, c, e, SpatialNormal::Free([0.0, 0.0, 1.0])),
        Err(SpatialError::InvalidValue(_))
    ));
}

/// A planar rectangle: 4 corners, 2 horizontal and 2 vertical lines,
/// width, height and a fixed corner. Returns the corners.
fn planar_rectangle(s: &mut System) -> Vec<mitcad_solver::PointId> {
    let p: Vec<_> = [[0.0, 0.0], [38.0, 1.0], [41.0, 19.0], [-1.0, 21.0]]
        .iter()
        .map(|q| s.add_point(q[0], q[1]))
        .collect();
    let l: Vec<_> = (0..4)
        .map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap())
        .collect();
    for c in [
        Constraint::Horizontal(l[0]),
        Constraint::Horizontal(l[2]),
        Constraint::Vertical(l[1]),
        Constraint::Vertical(l[3]),
        Constraint::Length {
            line: l[0],
            value: 40.0,
        },
        Constraint::Length {
            line: l[1],
            value: 20.0,
        },
        Constraint::FixPoint(p[0]),
    ] {
        s.add_constraint(c).unwrap();
    }
    p
}

/// Fixture 14: the converted rectangle. The planar relations keep their
/// rows on x and y (rank 8), each corner gets a membership row: 12
/// unknowns, rank 12, DOF 0, and the same corners as the planar solve.
/// Releasing one corner from the plane frees its z.
#[test]
fn converted_rectangle_keeps_its_planar_relations() {
    let mut planar = System::new();
    let pp = planar_rectangle(&mut planar);
    assert!(planar.solve(&SolveOptions::default()).is_ok());

    let mut s = SpatialSystem::new();
    let p: Vec<SpatialPointId> = [
        [0.0, 0.0, 0.2],
        [38.0, 1.0, -0.1],
        [41.0, 19.0, 0.3],
        [-1.0, 21.0, 0.0],
    ]
    .iter()
    .map(|&v| pt(&mut s, v))
    .collect();
    let l: Vec<_> = (0..4)
        .map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap())
        .collect();
    let e: Vec<_> = l
        .iter()
        .map(|&x| s.in_plane_curve(SpatialCurve::Line(x), Axis::Z).unwrap())
        .collect();
    let p0 = s.in_plane_point(p[0], Axis::Z).unwrap();
    let mut ids = Vec::new();
    for c in [
        Constraint::Horizontal(e[0]),
        Constraint::Horizontal(e[2]),
        Constraint::Vertical(e[1]),
        Constraint::Vertical(e[3]),
        Constraint::Length {
            line: e[0],
            value: 40.0,
        },
        Constraint::Length {
            line: e[1],
            value: 20.0,
        },
        Constraint::FixPoint(p0),
    ] {
        ids.push(s.add_in_plane(c).unwrap());
    }
    let groups = s.elevation_groups(&p);
    assert_eq!(groups, p);
    let mut member = Vec::new();
    for &q in &groups {
        member.push(s.add_constraint(OnSketchPlane(q)).unwrap());
    }
    solve_ok(&mut s);
    for (sp, &pl) in p.iter().zip(&pp) {
        let want = planar.point(pl).unwrap();
        assert_at(&s, *sp, [want[0], want[1], 0.0]);
    }
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (12, 12, 0));
    assert!(a.redundant.is_empty() && a.conflicts.is_empty() && a.issues.is_empty());
    assert!(a.is_fully_constrained());

    s.remove_constraint(member[2]).unwrap();
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (12, 11, 1));
    // The in-plane rows still hold the corner's projection.
    s.set_point(p[2], [40.0, 20.0, 7.0]).unwrap();
    solve_ok(&mut s);
    assert_at(&s, p[2], [40.0, 20.0, 7.0]);
    // An in-plane dimension changes like a planar one.
    let width = ids[4];
    assert!(matches!(
        s.in_plane_constraint(width),
        Some(Constraint::Length { .. })
    ));
    s.set_value(width, 50.0).unwrap();
    solve_ok(&mut s);
    assert_at(&s, p[1], [50.0, 0.0, 0.0]);
    assert!((s.measure(width).unwrap() - 50.0).abs() < EPS);
    // The planar direction graph sees the in-plane relations: a second
    // vertical on a horizontal line contradicts them.
    let bad = s.add_in_plane(Constraint::Vertical(e[0])).unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SpatialStatus::Conflicting);
    assert!(r.conflicting().contains(&bad));
    // A line an in-plane relation uses stays.
    assert_eq!(s.remove_line(l[0]), Err(SpatialError::InUse));
}

/// Fixture 15: a converted arc whose start is coincident with a line end:
/// the elevation groups are the arc's points with the line end, and the
/// line's other end, so two membership rows and no dependent one; a third
/// on the joined line end would be redundant.
#[test]
fn converted_arc_with_a_coincident_line_end() {
    let mut s = SpatialSystem::new();
    let c = pt(&mut s, [0.0, 0.0, 0.2]);
    let st = pt(&mut s, [10.0, 0.0, -0.1]);
    let e = pt(&mut s, [0.0, 10.0, 0.3]);
    let q = pt(&mut s, [10.1, 0.0, 0.0]);
    let r = pt(&mut s, [20.0, 0.0, 0.5]);
    s.add_arc(c, st, e, SpatialNormal::Axis(Axis::Z, Positive))
        .unwrap();
    s.add_line(q, r).unwrap();
    s.add_constraint(Coincident(st, q)).unwrap();
    let groups = s.elevation_groups(&[c, st, e, q, r]);
    assert_eq!(groups, vec![c, r]);
    for &g in &groups {
        s.add_constraint(OnSketchPlane(g)).unwrap();
    }
    solve_ok(&mut s);
    for p in [c, st, e, q, r] {
        assert!(s.point(p).unwrap()[2].abs() < EPS);
    }
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (15, 8, 7));
    assert!(a.redundant.is_empty() && a.conflicts.is_empty() && a.issues.is_empty());
    let extra = s.add_constraint(OnSketchPlane(q)).unwrap();
    let a = s.analyze();
    assert_eq!(a.rank, 8);
    assert!(a.redundant_constraints().contains(&extra));
}

/// mitcad#1550: a plane-bound ellipse in the YZ plane solves its minor
/// radius: free it has 6 degrees of freedom (centre, the major point in the
/// plane, the minor radius); a point on it held where the ellipse should
/// pass sets the minor radius, and the major and minor radius dimensions
/// drive it.
#[test]
fn plane_bound_ellipse_solves_its_minor_radius() {
    let mut s = SpatialSystem::new();
    let c = pt(&mut s, [5.0, 0.0, 0.0]);
    let a = pt(&mut s, [5.0, 20.0, 0.3]);
    let e = s.add_ellipse(c, a, 4.0, Axis::X, None).unwrap();
    solve_ok(&mut s);
    // The major point went into the plane through the centre.
    assert!((s.point(a).unwrap()[0] - 5.0).abs() < EPS);
    let a0 = s.analyze();
    assert_eq!((a0.unknowns, a0.dof), (7, 6));
    fix(&mut s, c);
    // The major axis along Y, 20 mm long.
    s.add_constraint(Offset {
        a: c,
        b: a,
        axis: Axis::Z,
        value: 0.0,
    })
    .unwrap();
    let m = s
        .add_constraint(EllipseRadius {
            curve: e,
            minor: false,
            value: 20.0,
        })
        .unwrap();
    // A point on the ellipse along its minor axis, 7 mm from the centre:
    // the minor radius becomes 7.
    let q = pt(&mut s, [5.0, 0.0, 6.0]);
    s.add_constraint(PointOn {
        point: q,
        curve: SpatialCurve::Curve(e),
    })
    .unwrap();
    for (axis, value) in [(Axis::Y, 0.0), (Axis::Z, 7.0)] {
        s.add_constraint(Offset {
            a: c,
            b: q,
            axis,
            value,
        })
        .unwrap();
    }
    solve_ok(&mut s);
    let (major, minor) = s.ellipse_radii(e).unwrap();
    assert!((major - 20.0).abs() < EPS, "major {major}");
    assert!((minor - 7.0).abs() < EPS, "minor {minor}");
    assert!((s.measure(m).unwrap() - 20.0).abs() < EPS);
    assert_at(&s, q, [5.0, 0.0, 7.0]);
    let a1 = s.analyze();
    assert_eq!(a1.dof, 0);
    assert!(a1.redundant.is_empty() && a1.conflicts.is_empty() && a1.issues.is_empty());
    // A minor radius dimension that contradicts the held point conflicts.
    let bad = s
        .add_constraint(EllipseRadius {
            curve: e,
            minor: true,
            value: 9.0,
        })
        .unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SpatialStatus::Conflicting);
    assert!(r.conflicting().contains(&bad));
    s.remove_constraint(bad).unwrap();
    // Relations an ellipse does not take are refused.
    assert!(
        s.add_constraint(Radius {
            curve: e,
            value: 3.0
        })
        .is_err()
    );
}

/// mitcad#1550: in-plane relations of the plane square to X: a rectangle
/// drawn on the YZ plane keeps its sides along Y and Z with horizontal and
/// vertical relations of that plane, its corners held in one plane by
/// offsets along X.
#[test]
fn in_plane_relations_of_the_yz_plane() {
    let mut s = SpatialSystem::new();
    let p: Vec<SpatialPointId> = [
        [3.0, 0.0, 0.0],
        [3.1, 40.0, 0.5],
        [2.9, 41.0, 20.0],
        [3.0, -1.0, 19.0],
    ]
    .iter()
    .map(|&v| pt(&mut s, v))
    .collect();
    let l: Vec<_> = (0..4)
        .map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap())
        .collect();
    let e: Vec<_> = l
        .iter()
        .map(|&x| s.in_plane_curve(SpatialCurve::Line(x), Axis::X).unwrap())
        .collect();
    for c in [
        Constraint::Horizontal(e[0]),
        Constraint::Vertical(e[1]),
        Constraint::Horizontal(e[2]),
        Constraint::Vertical(e[3]),
    ] {
        s.add_in_plane(c).unwrap();
    }
    for &q in &p[1..] {
        s.add_constraint(Offset {
            a: p[0],
            b: q,
            axis: Axis::X,
            value: 0.0,
        })
        .unwrap();
    }
    solve_ok(&mut s);
    let at: Vec<[f64; 3]> = p.iter().map(|&q| s.point(q).unwrap()).collect();
    for v in &at {
        assert!((v[0] - at[0][0]).abs() < EPS);
    }
    // Horizontal in YZ: along Y (same z); vertical: along Z (same y).
    assert!((at[0][2] - at[1][2]).abs() < EPS && (at[2][2] - at[3][2]).abs() < EPS);
    assert!((at[1][1] - at[2][1]).abs() < EPS && (at[3][1] - at[0][1]).abs() < EPS);
    let a = s.analyze();
    // Twelve coordinates, four in-plane rows and three offsets.
    assert_eq!((a.unknowns, a.rank, a.dof), (12, 7, 5));
    assert!(a.redundant.is_empty() && a.conflicts.is_empty());
    // The same line's stand-in in another plane is another entity.
    let other = s.in_plane_curve(SpatialCurve::Line(l[0]), Axis::Z).unwrap();
    assert_ne!(other, e[0]);
}
