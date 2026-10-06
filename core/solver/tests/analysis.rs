// SPDX-License-Identifier: MIT
//! Degrees of freedom, fully constrained status, redundant and conflicting
//! constraints.

mod common;

use common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::{ConstraintId, SolveStatus, SplineEnd, System};
use std::f64::consts::PI;

#[test]
fn dof_of_free_entities() {
    let mut s = System::new();
    assert_eq!(s.analyze().dof, 0);
    s.add_point(0.0, 0.0);
    assert_eq!(s.analyze().dof, 2);

    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(1.0, 0.0);
    s.add_line(a, b).unwrap();
    assert_eq!(s.analyze().dof, 4, "unconstrained line");

    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    s.add_circle(c, 2.0).unwrap();
    assert_eq!(s.analyze().dof, 3, "circle");

    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let p = s.add_point(2.0, 0.0);
    let q = s.add_point(0.0, 2.0);
    s.add_arc(c, p, q).unwrap();
    assert_eq!(s.analyze().dof, 5, "arc");

    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let a = s.add_point(4.0, 0.0);
    s.add_ellipse(c, a, 2.0).unwrap();
    assert_eq!(s.analyze().dof, 5, "ellipse");

    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let a = s.add_point(4.0, 0.0);
    let p = s.add_point(4.0, 0.0);
    let q = s.add_point(0.0, 2.0);
    s.add_elliptical_arc(c, a, 2.0, p, q).unwrap();
    assert_eq!(s.analyze().dof, 7, "elliptical arc");

    let mut s = System::new();
    let ctrl: Vec<_> = (0..5)
        .map(|i| s.add_point(i as f64, (i % 2) as f64))
        .collect();
    s.add_bspline(3, &ctrl, None, None).unwrap();
    assert_eq!(s.analyze().dof, 10, "B-spline: two per control point");

    let mut s = System::new();
    let fit: Vec<_> = (0..4)
        .map(|i| s.add_point(i as f64 * 3.0, (i % 2) as f64))
        .collect();
    s.add_fitted_spline(&fit, [SplineEnd::Natural; 2]).unwrap();
    assert_eq!(s.analyze().dof, 8, "fitted spline: two per fit point");

    let mut s = System::new();
    let fit: Vec<_> = (0..4)
        .map(|i| s.add_point(i as f64 * 3.0, (i % 2) as f64))
        .collect();
    s.add_fitted_spline(&fit, [SplineEnd::Free, SplineEnd::Natural])
        .unwrap();
    assert_eq!(s.analyze().dof, 10, "free end tangent: two more");
}

#[test]
fn dof_removed_by_single_constraints() {
    // Each constraint removes as many freedoms as it has equations.
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 1.0);
    let line = s.add_line(a, b).unwrap();
    s.add_constraint(FixEntity(line)).unwrap();
    let c = s.add_point(0.0, 10.0);
    let circle = s.add_circle(c, 3.0).unwrap();
    s.add_constraint(FixEntity(circle)).unwrap();
    let ctrl: Vec<_> = (0..4)
        .map(|i| s.add_point(20.0 + i as f64, (i % 2) as f64))
        .collect();
    let sp = s.add_bspline(3, &ctrl, None, None).unwrap();
    s.add_constraint(FixEntity(sp)).unwrap();
    assert_eq!(s.analyze().dof, 0);
    for e in [line, circle, sp] {
        let p = s.add_point(5.0, 5.0);
        s.add_constraint(PointOnCurve(p, e)).unwrap();
        let r = s.solve(&opts());
        assert!(r.is_ok(), "{e:?}: {r:?}");
    }
    assert_eq!(
        s.analyze().dof,
        3,
        "a point on a fixed curve slides along it"
    );
    // A circle tangent to the fixed line, with a radius: its centre slides.
    let c2 = s.add_point(30.0, 5.0);
    let tc = s.add_circle(c2, 2.0).unwrap();
    s.add_constraint(Tangent(line, tc)).unwrap();
    s.add_constraint(Radius {
        entity: tc,
        value: 2.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_eq!(s.analyze().dof, 4);
}

#[test]
fn symmetric_arc_follows_its_mirror_image() {
    let mut s = System::new();
    let p = s.add_point(0.0, 0.0);
    let q = s.add_point(0.0, 10.0);
    let axis = s.add_line(p, q).unwrap();
    s.add_constraint(FixEntity(axis)).unwrap();
    let arc_a = {
        let c = s.add_point(-5.0, 5.0);
        let st = s.add_point(-2.0, 5.0);
        let en = s.add_point(-5.0, 8.0);
        s.add_arc(c, st, en).unwrap()
    };
    let arc_b = {
        let c = s.add_point(5.5, 5.0);
        let st = s.add_point(5.0, 8.5);
        let en = s.add_point(2.0, 4.5);
        s.add_arc(c, st, en).unwrap()
    };
    s.add_constraint(SymmetricEntities {
        a: arc_a,
        b: arc_b,
        axis,
    })
    .unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!(an.dof, 5, "only arc a is free");
    assert!(an.redundant.is_empty());
}

#[test]
fn horizontal_line_with_fixed_start_and_length_is_fully_constrained() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(8.0, 1.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(FixPoint(a)).unwrap();
    let an = s.analyze();
    assert_eq!(an.dof, 2);
    assert!(an.is_point_fully_constrained(a));
    assert!(!an.is_point_fully_constrained(b));
    assert!(!an.is_entity_fully_constrained(l));
    s.add_constraint(Horizontal(l)).unwrap();
    s.add_constraint(Length {
        line: l,
        value: 10.0,
    })
    .unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!(an.dof, 0);
    assert!(an.is_point_fully_constrained(b));
    assert!(an.is_entity_fully_constrained(l));
    assert!(an.redundant.is_empty() && an.conflicts.is_empty());
}

#[test]
fn rectangle_dof_and_status() {
    let mut s = System::new();
    let r = rect(&mut s, 0.0, 0.0, 10.0, 5.0);
    let an = s.analyze();
    assert_eq!(an.dof, 4, "position and size");
    assert!(an.fully_constrained_points.is_empty());
    let w = s
        .add_constraint(Length {
            line: r.l[0],
            value: 12.0,
        })
        .unwrap();
    s.add_constraint(Length {
        line: r.l[1],
        value: 6.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_eq!(s.analyze().dof, 2, "only translation left");
    s.add_constraint(FixPoint(r.p[0])).unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!(an.dof, 0);
    assert_eq!(an.fully_constrained_entities.len(), 4);
    assert_eq!(an.fully_constrained_points.len(), 4);
    assert_eq!(s.measure(w), Some(12.0));
}

#[test]
fn horizontal_and_vertical_on_one_line_conflict() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 1.0);
    let l = s.add_line(a, b).unwrap();
    let h = s.add_constraint(Horizontal(l)).unwrap();
    solve_ok(&mut s);
    let v = s.add_constraint(Vertical(l)).unwrap();
    let before = (pt(&s, a), pt(&s, b));
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting);
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(r.conflicts[0].constraint, v);
    assert_eq!(r.conflicting(), vec![h, v]);
    assert_eq!((pt(&s, a), pt(&s, b)), before, "geometry left alone");
    let an = s.analyze();
    assert_eq!(an.conflicting_constraints(), vec![h, v]);
}

#[test]
fn horizontal_and_vertical_points_of_a_line_conflict() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 1.0);
    let l = s.add_line(a, b).unwrap();
    let h = s.add_constraint(HorizontalPoints(b, a)).unwrap();
    let v = s.add_constraint(Vertical(l)).unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.conflicting(), vec![h, v]);
}

#[test]
fn parallel_and_perpendicular_conflict() {
    let mut s = System::new();
    let p: Vec<_> = (0..4)
        .map(|i| s.add_point(i as f64, (i * i) as f64))
        .collect();
    let a = s.add_line(p[0], p[1]).unwrap();
    let b = s.add_line(p[2], p[3]).unwrap();
    let c1 = s.add_constraint(Parallel(a, b)).unwrap();
    let c2 = s.add_constraint(Perpendicular(b, a)).unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting);
    assert_eq!(r.conflicting(), vec![c1, c2]);
}

#[test]
fn direction_cycle_conflict_through_several_lines() {
    let mut s = System::new();
    let p: Vec<_> = (0..6)
        .map(|i| s.add_point(i as f64, (i * i) as f64 * 0.1))
        .collect();
    let a = s.add_line(p[0], p[1]).unwrap();
    let b = s.add_line(p[2], p[3]).unwrap();
    let c = s.add_line(p[4], p[5]).unwrap();
    let h = s.add_constraint(Horizontal(a)).unwrap();
    let par = s.add_constraint(Parallel(a, b)).unwrap();
    let _unrelated = s
        .add_constraint(Length {
            line: c,
            value: 3.0,
        })
        .unwrap();
    let ang = s
        .add_constraint(Angle {
            a: b,
            b: c,
            value: PI / 6.0,
        })
        .unwrap();
    solve_ok(&mut s);
    let v = s.add_constraint(Vertical(c)).unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.conflicting(), vec![h, par, ang, v]);
    assert_eq!(r.conflicts[0].constraint, v);
}

#[test]
fn contradictory_distances_conflict() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    let c = s.add_point(9.0, 9.0);
    let free = s
        .add_constraint(Distance {
            a: c,
            b: a,
            value: 3.0,
        })
        .unwrap();
    let d1 = s.add_constraint(Distance { a, b, value: 10.0 }).unwrap();
    let d2 = s
        .add_constraint(Distance {
            a: b,
            b: a,
            value: 20.0,
        })
        .unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting);
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(r.conflicts[0].constraint, d2);
    assert_eq!(r.conflicts[0].involved, vec![d1, d2]);
    assert!(!r.conflicting().contains(&free));
}

#[test]
fn distance_between_fixed_points_conflicts() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    let f1 = s.add_constraint(FixPoint(a)).unwrap();
    let f2 = s.add_constraint(FixPoint(b)).unwrap();
    let d = s.add_constraint(Distance { a, b, value: 8.0 }).unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.conflicting(), vec![f1, f2, d]);
    assert_pt(&s, b, 5.0, 0.0);
}

#[test]
fn conflict_in_one_part_does_not_block_the_others() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(5.0, 0.0);
    s.add_constraint(Distance { a, b, value: 10.0 }).unwrap();
    s.add_constraint(Distance { a, b, value: 20.0 }).unwrap();
    let c = s.add_point(0.0, 10.0);
    let d = s.add_point(3.0, 10.0);
    s.add_constraint(Distance {
        a: c,
        b: d,
        value: 4.0,
    })
    .unwrap();
    let r = s.solve(&opts());
    assert_eq!(r.status, SolveStatus::Conflicting);
    assert_near(dist(pt(&s, c), pt(&s, d)), 4.0, "independent part solved");
    assert_eq!(pt(&s, b), [5.0, 0.0]);
}

#[test]
fn redundant_constraints_are_accepted_and_reported() {
    let mut s = System::new();
    let r = rect(&mut s, 0.0, 0.0, 10.0, 5.0);
    s.add_constraint(FixPoint(r.p[0])).unwrap();
    s.add_constraint(Length {
        line: r.l[0],
        value: 10.0,
    })
    .unwrap();
    s.add_constraint(Length {
        line: r.l[1],
        value: 5.0,
    })
    .unwrap();
    // Opposite sides are parallel already; the top length follows too.
    let par = s.add_constraint(Parallel(r.l[0], r.l[2])).unwrap();
    let top = s
        .add_constraint(Length {
            line: r.l[2],
            value: 10.0,
        })
        .unwrap();
    let dup = s.add_constraint(Horizontal(r.l[0])).unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!(an.redundant_constraints(), vec![par, top, dup]);
    assert!(an.conflicts.is_empty());
    assert_eq!(an.dof, 0);
    // The redundant set of the top length: the other dimensions and
    // directions that imply it.
    let dep = an.redundant.iter().find(|d| d.constraint == top).unwrap();
    assert!(
        dep.involved.contains(&top) && dep.involved.len() > 2,
        "{dep:?}"
    );
}

#[test]
fn redundant_but_consistent_solves_from_imperfect_geometry() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(9.0, 1.0);
    s.add_constraint(FixPoint(a)).unwrap();
    s.add_constraint(Distance { a, b, value: 10.0 }).unwrap();
    s.add_constraint(Distance {
        a: b,
        b: a,
        value: 10.0,
    })
    .unwrap();
    s.add_constraint(HorizontalPoints(a, b)).unwrap();
    solve_ok(&mut s);
    assert_pt(&s, b, 10.0, 0.0);
    assert_eq!(s.analyze().redundant.len(), 1);
}

#[test]
fn coincident_with_itself_is_redundant() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let c = s.add_constraint(Coincident(a, a)).unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!(an.redundant_constraints(), vec![c]);
    assert_eq!(an.dof, 2);
}

#[test]
fn no_solution_without_contradiction_reports_not_converged() {
    // Triangle inequality violated: |ab| = 10, |bc| = 1, |ca| = 1.
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 0.0);
    let c = s.add_point(5.0, 1.0);
    s.add_constraint(Distance { a, b, value: 10.0 }).unwrap();
    s.add_constraint(Distance {
        a: b,
        b: c,
        value: 1.0,
    })
    .unwrap();
    s.add_constraint(Distance {
        a: c,
        b: a,
        value: 1.0,
    })
    .unwrap();
    let r = s.solve(&opts());
    assert_ne!(r.status, SolveStatus::Converged);
    assert!(r.residual > 1e-3);
    assert_eq!(pt(&s, c), [5.0, 1.0], "left where it was");
}

#[test]
fn removing_the_conflicting_constraint_solves_again() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 1.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(Horizontal(l)).unwrap();
    let v: ConstraintId = s.add_constraint(Vertical(l)).unwrap();
    assert!(!s.solve(&opts()).is_ok());
    s.remove_constraint(v).unwrap();
    solve_ok(&mut s);
}

#[test]
fn rotated_copy_of_an_arc_and_a_circle_follows_the_original() {
    // Copies bound to originals add no freedom and are never redundant,
    // even for an arc end, which its arc keeps on the circle already.
    let mut s = System::new();
    let center = s.add_point(0.0, 0.0);
    s.add_constraint(FixPoint(center)).unwrap();
    let (c, st, en) = (
        s.add_point(10.0, 0.0),
        s.add_point(13.0, 0.0),
        s.add_point(10.0, 3.0),
    );
    let arc = s.add_arc(c, st, en).unwrap();
    let k = s.add_point(20.0, 0.0);
    let circle = s.add_circle(k, 2.0).unwrap();
    let angle = PI / 3.0;
    // Rough copies, as a pattern makes them before the solve.
    let (c2, st2, en2) = (
        s.add_point(5.2, 8.6),
        s.add_point(6.4, 11.4),
        s.add_point(2.4, 10.3),
    );
    let arc2 = s.add_arc(c2, st2, en2).unwrap();
    let k2 = s.add_point(10.1, 17.3);
    let circle2 = s.add_circle(k2, 2.5).unwrap();
    for (a, b) in [(c, c2), (st, st2), (k, k2)] {
        s.add_constraint(Rotated {
            center,
            a,
            b,
            angle,
        })
        .unwrap();
    }
    s.add_constraint(TurnedDirection {
        a_center: c,
        a: en,
        b_center: c2,
        b: en2,
        angle,
    })
    .unwrap();
    s.add_constraint(EqualSize(circle, circle2)).unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!(an.dof, 5 + 3, "only the originals are free");
    assert!(an.redundant.is_empty(), "{:?}", an.redundant);
    let turn = |p: [f64; 2]| {
        let (sn, cs) = angle.sin_cos();
        [cs * p[0] - sn * p[1], sn * p[0] + cs * p[1]]
    };
    for (a, b) in [(c, c2), (st, st2), (en, en2), (k, k2)] {
        let (want, got) = (turn(pt(&s, a)), pt(&s, b));
        assert!((want[0] - got[0]).abs() < 1e-9 && (want[1] - got[1]).abs() < 1e-9);
    }
    assert!((s.radius(circle2).unwrap() - s.radius(circle).unwrap()).abs() < 1e-9);
    assert!((s.radius(arc2).unwrap() - s.radius(arc).unwrap()).abs() < 1e-9);
    // The original changes: the copy follows.
    s.add_constraint(Radius {
        entity: circle,
        value: 4.0,
    })
    .unwrap();
    s.add_constraint(Radius {
        entity: arc,
        value: 5.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert!((s.radius(circle2).unwrap() - 4.0).abs() < 1e-9);
    assert!((s.radius(arc2).unwrap() - 5.0).abs() < 1e-9);
}

#[test]
fn translated_copies_of_an_ellipse_keep_its_size() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let m = s.add_point(6.0, 0.0);
    let e = s.add_ellipse(c, m, 3.0).unwrap();
    let c2 = s.add_point(10.5, 0.2);
    let m2 = s.add_point(16.0, 0.0);
    let e2 = s.add_ellipse(c2, m2, 2.0).unwrap();
    for (a, b) in [(c, c2), (m, m2)] {
        s.add_constraint(Translated {
            a,
            b,
            by: [10.0, 0.0],
        })
        .unwrap();
    }
    s.add_constraint(EqualSize(e, e2)).unwrap();
    s.add_constraint(MinorRadius {
        ellipse: e,
        value: 2.5,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_eq!(s.analyze().dof, 4);
    assert!((s.minor_radius(e2).unwrap() - 2.5).abs() < 1e-9);
    assert!((pt(&s, m2)[0] - pt(&s, m)[0] - 10.0).abs() < 1e-9);
    let line = s.add_line(c, m).unwrap();
    assert!(s.add_constraint(EqualSize(e, line)).is_err());
}
