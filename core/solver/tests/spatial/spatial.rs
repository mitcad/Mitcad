// SPDX-License-Identifier: MIT
//! The spatial point/line system of 3D sketches (mitcad#1520): the
//! analytical fixtures of the solver study as executable cases, checking
//! statuses and coordinates (1e-6 mm), plus planar regression cases for
//! the component helper the two systems share.

use mitcad_solver::AxisSense::{Negative, Positive};
use mitcad_solver::SpatialConstraint::*;
use mitcad_solver::{
    Axis, SPATIAL_MIN_SEGMENT, SpatialConstraintId, SpatialDragGoal, SpatialError, SpatialIssue,
    SpatialLineId, SpatialOptions, SpatialPointId, SpatialSolveResult, SpatialStatus,
    SpatialSystem,
};

/// Geometric comparison tolerance (mm).
const EPS: f64 = 1e-6;
/// Where a soft drag goal ends (mm): the drag stops within the solve
/// tolerance of its goal; the relations themselves are checked at `EPS`.
const DRAG_EPS: f64 = 1e-5;

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

fn assert_len(s: &SpatialSystem, l: SpatialLineId, want: f64) {
    let v = s.line_length(l).expect("line");
    assert!(
        (v - want).abs() < EPS,
        "line {l:?} is {v} long, expected {want}"
    );
}

fn drag(s: &mut SpatialSystem, point: SpatialPointId, target: [f64; 3]) -> SpatialSolveResult {
    s.drag(&[SpatialDragGoal { point, target }], &opts())
        .expect("valid goal")
}

/// The study's reference route: A=(0,0,0), B=(40,0,0), C=(40,30,0),
/// D=(40,30,20) (moved by `at`), four shared points, three lines aligned
/// with X, Y and Z in the positive sense, lengths 40, 30 and 20, A attached
/// at `at`. Drawn imperfectly.
struct Route {
    p: [SpatialPointId; 4],
    l: [SpatialLineId; 3],
    origin: SpatialConstraintId,
    align: [SpatialConstraintId; 3],
    len: [SpatialConstraintId; 3],
}

fn route(s: &mut SpatialSystem, at: [f64; 3]) -> Route {
    let off = |v: [f64; 3]| [v[0] + at[0], v[1] + at[1], v[2] + at[2]];
    let p = [
        s.add_point(off([0.3, -0.2, 0.1])).unwrap(),
        s.add_point(off([38.0, 1.0, -0.5])).unwrap(),
        s.add_point(off([41.0, 28.0, 0.7])).unwrap(),
        s.add_point(off([39.5, 31.0, 18.0])).unwrap(),
    ];
    let l = [
        s.add_line(p[0], p[1]).unwrap(),
        s.add_line(p[1], p[2]).unwrap(),
        s.add_line(p[2], p[3]).unwrap(),
    ];
    let origin = s
        .add_constraint(Attach {
            point: p[0],
            target: at,
        })
        .unwrap();
    let axes = [Axis::X, Axis::Y, Axis::Z];
    let align = [0, 1, 2].map(|i| {
        s.add_constraint(Align {
            line: l[i],
            axis: axes[i],
            sense: Positive,
        })
        .unwrap()
    });
    let values = [40.0, 30.0, 20.0];
    let len = [0, 1, 2].map(|i| {
        s.add_constraint(Length {
            line: l[i],
            value: values[i],
        })
        .unwrap()
    });
    Route {
        p,
        l,
        origin,
        align,
        len,
    }
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[test]
fn one_point_free_fixed_and_unfixed() {
    let mut s = SpatialSystem::new();
    let p = s.add_point([1.0, 2.0, 3.0]).unwrap();
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (3, 0, 3));
    assert!(!a.is_fully_constrained());
    assert!(a.fully_constrained_points.is_empty());

    let fix = s
        .add_constraint(Fix {
            point: p,
            at: [1.0, 2.0, 3.0],
        })
        .unwrap();
    solve_ok(&mut s);
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (3, 3, 0));
    assert!(a.is_fully_constrained());
    assert!(a.is_point_fully_constrained(p));

    // Unfix: nothing else determines it.
    s.remove_constraint(fix).unwrap();
    solve_ok(&mut s);
    let a = s.analyze();
    assert_eq!((a.rank, a.dof), (0, 3));
    assert!(!a.is_point_fully_constrained(p));
    assert_at(&s, p, [1.0, 2.0, 3.0]);
}

#[test]
fn two_points_with_a_length_then_one_fixed() {
    let mut s = SpatialSystem::new();
    let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
    let b = s.add_point([10.0, 0.0, 0.0]).unwrap();
    let l = s.add_line(a, b).unwrap();
    assert_eq!(s.analyze().dof, 6);
    s.add_constraint(Length {
        line: l,
        value: 10.0,
    })
    .unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!((an.unknowns, an.rank, an.dof), (6, 1, 5));

    // A fixed: B lies on a sphere.
    s.add_constraint(Fix {
        point: a,
        at: [0.0; 3],
    })
    .unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!((an.rank, an.dof), (4, 2));
    assert!(an.is_point_fully_constrained(a));
    assert!(!an.is_point_fully_constrained(b));
    assert!(!an.is_line_fully_constrained(l));
}

#[test]
fn shared_identity_and_explicit_coincidence_differ() {
    // Separate points joined by a coincidence: three rows, a free common
    // place.
    let mut s = SpatialSystem::new();
    let p = s.add_point([1.0, 2.0, 3.0]).unwrap();
    let q = s.add_point([1.5, 2.5, 2.0]).unwrap();
    let c = s.add_constraint(Coincident(p, q)).unwrap();
    solve_ok(&mut s);
    assert!(near(s.point(p).unwrap(), s.point(q).unwrap(), EPS));
    let an = s.analyze();
    assert_eq!((an.unknowns, an.rank, an.dof), (6, 3, 3));
    // Removing the coincidence separates them again.
    s.remove_constraint(c).unwrap();
    assert_eq!(s.analyze().dof, 6);

    // One shared point: three unknowns, no rows, and it stays in use while
    // a line refers to it.
    let mut s = SpatialSystem::new();
    let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
    let b = s.add_point([10.0, 0.0, 0.0]).unwrap();
    let c = s.add_point([10.0, 10.0, 5.0]).unwrap();
    let ab = s.add_line(a, b).unwrap();
    let bc = s.add_line(b, c).unwrap();
    let an = s.analyze();
    assert_eq!((an.unknowns, an.rank, an.dof), (9, 0, 9));
    assert_eq!(s.remove_point(b), Err(SpatialError::InUse));
    s.remove_line(ab).unwrap();
    assert_eq!(s.line(bc), Some([b, c]));
    assert_eq!(s.remove_point(b), Err(SpatialError::InUse));
    s.remove_line(bc).unwrap();
    s.remove_point(b).unwrap();
    assert_eq!(s.analyze().unknowns, 6);
    assert_eq!(s.points().collect::<Vec<_>>(), vec![a, c]);
}

#[test]
fn reference_route_is_fully_constrained_and_not_planar() {
    let mut s = SpatialSystem::new();
    let r = route(&mut s, [0.0; 3]);
    solve_ok(&mut s);
    let want = [
        [0.0, 0.0, 0.0],
        [40.0, 0.0, 0.0],
        [40.0, 30.0, 0.0],
        [40.0, 30.0, 20.0],
    ];
    for (p, w) in r.p.iter().zip(want) {
        assert_at(&s, *p, w);
    }
    let a = s.analyze();
    assert_eq!((a.unknowns, a.rank, a.dof), (12, 12, 0));
    assert!(a.is_fully_constrained());
    assert!(a.redundant.is_empty() && a.conflicts.is_empty() && a.issues.is_empty());
    assert_eq!(a.fully_constrained_points, r.p.to_vec());
    assert_eq!(a.fully_constrained_lines, r.l.to_vec());
    // Not coplanar: the triple product of B-A, C-A, D-A.
    let pt = |i: usize| s.point(r.p[i]).unwrap();
    let (b, c, d) = (pt(1), pt(2), pt(3));
    let triple = b[0] * (c[1] * d[2] - c[2] * d[1]) - b[1] * (c[0] * d[2] - c[2] * d[0])
        + b[2] * (c[0] * d[1] - c[1] * d[0]);
    assert!((triple - 24000.0).abs() < 1e-3, "{triple}");
    // Driven measurements read the geometry and add no rows.
    assert_len(&s, r.l[1], 30.0);
    assert!((s.offset(r.p[0], r.p[3], Axis::Z).unwrap() - 20.0).abs() < EPS);
    assert!((s.measure(r.len[2]).unwrap() - 20.0).abs() < EPS);
    assert_eq!(s.analyze().rank, 12);
}

#[test]
fn dimension_edit_then_removal_leaves_one_freedom() {
    let mut s = SpatialSystem::new();
    let r = route(&mut s, [0.0; 3]);
    solve_ok(&mut s);
    s.set_value(r.len[1], 50.0).unwrap();
    solve_ok(&mut s);
    assert_at(&s, r.p[1], [40.0, 0.0, 0.0]);
    assert_at(&s, r.p[2], [40.0, 50.0, 0.0]);
    assert_at(&s, r.p[3], [40.0, 50.0, 20.0]);
    assert_eq!(s.analyze().rank, 12);

    // Without BC's length, C and D share one free Y translation.
    s.remove_constraint(r.len[1]).unwrap();
    solve_ok(&mut s);
    assert_at(&s, r.p[2], [40.0, 50.0, 0.0]);
    let a = s.analyze();
    assert_eq!((a.rank, a.dof), (11, 1));
    assert!(!a.is_fully_constrained());
    assert_eq!(a.fully_constrained_points, vec![r.p[0], r.p[1]]);
    assert_eq!(a.fully_constrained_lines, vec![r.l[0]]);

    // Dragging C moves C and D along Y together; the rest holds.
    let res = drag(&mut s, r.p[2], [40.0, 70.0, 0.0]);
    assert!(res.is_ok(), "{res:?}");
    let c = s.point(r.p[2]).unwrap();
    assert!(near(c, [40.0, 70.0, 0.0], DRAG_EPS), "{c:?}");
    assert_at(&s, r.p[3], [40.0, c[1], 20.0]);
    assert_at(&s, r.p[1], [40.0, 0.0, 0.0]);

    // Past B, BC would point against its sense: refused, nothing moves.
    let before: Vec<[f64; 3]> = r.p.iter().map(|&p| s.point(p).unwrap()).collect();
    let res = drag(&mut s, r.p[2], [40.0, -5.0, 0.0]);
    assert_eq!(res.status, SpatialStatus::Invalid);
    assert!(res.issues.contains(&SpatialIssue::ReversedSense {
        constraint: r.align[1],
        line: r.l[1],
    }));
    for (p, w) in r.p.iter().zip(&before) {
        assert_at(&s, *p, *w);
    }

    // At t = 0 the line BC collapses: invalid geometry, not a solution.
    s.set_point(r.p[2], [40.0, 0.0, 0.0]).unwrap();
    s.set_point(r.p[3], [40.0, 0.0, 20.0]).unwrap();
    let res = s.solve(&opts());
    assert_eq!(res.status, SpatialStatus::Invalid);
    assert!(res.issues.contains(&SpatialIssue::CollapsedLine(r.l[1])));
}

#[test]
fn without_the_origin_the_aligned_route_keeps_three_translations() {
    let mut s = SpatialSystem::new();
    let r = route(&mut s, [0.0; 3]);
    solve_ok(&mut s);
    s.remove_constraint(r.origin).unwrap();
    solve_ok(&mut s);
    let a = s.analyze();
    // Its directions are tied to the frame: no rotations remain.
    assert_eq!((a.unknowns, a.rank, a.dof), (12, 9, 3));
    assert!(a.fully_constrained_points.is_empty());
}

#[test]
fn a_repeated_length_is_redundant_and_a_different_one_conflicts() {
    let mut s = SpatialSystem::new();
    let r = route(&mut s, [0.0; 3]);
    solve_ok(&mut s);
    let again = s
        .add_constraint(Length {
            line: r.l[0],
            value: 40.0,
        })
        .unwrap();
    solve_ok(&mut s);
    let a = s.analyze();
    assert_eq!((a.rank, a.dof), (12, 0));
    assert_eq!(a.redundant_constraints(), vec![again]);
    assert!(a.redundant[0].involved.contains(&r.len[0]));
    assert!(a.conflicts.is_empty());
    assert!(a.is_fully_constrained());

    // 41 instead: no configuration satisfies both; nothing moves.
    s.remove_constraint(again).unwrap();
    let other = s
        .add_constraint(Length {
            line: r.l[0],
            value: 41.0,
        })
        .unwrap();
    let res = s.solve(&opts());
    assert_eq!(res.status, SpatialStatus::Conflicting, "{res:?}");
    let set = res.conflicting();
    assert!(set.contains(&r.len[0]) && set.contains(&other), "{set:?}");
    assert!(res.conflicts.iter().any(|d| d.constraint == other));
    assert_at(&s, r.p[1], [40.0, 0.0, 0.0]);
    assert_at(&s, r.p[3], [40.0, 30.0, 20.0]);
    let a = s.analyze();
    assert!(a.conflicting_constraints().contains(&other));
    assert!(!a.solved && !a.is_fully_constrained());
    assert!(a.fully_constrained_points.is_empty());
}

#[test]
fn two_attached_endpoints_with_a_length() {
    let mut s = SpatialSystem::new();
    let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
    let b = s.add_point([10.0, 0.0, 0.0]).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(Attach {
        point: a,
        target: [0.0; 3],
    })
    .unwrap();
    let at_b = s
        .add_constraint(Attach {
            point: b,
            target: [10.0, 0.0, 0.0],
        })
        .unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!((an.rank, an.dof), (6, 0));

    // A length of 10 follows from the attachments.
    let len = s
        .add_constraint(Length {
            line: l,
            value: 10.0,
        })
        .unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!((an.rank, an.dof), (6, 0));
    assert_eq!(an.redundant_constraints(), vec![len]);
    assert!(an.is_fully_constrained());

    // 12 contradicts them.
    s.set_value(len, 12.0).unwrap();
    let res = s.solve(&opts());
    assert_eq!(res.status, SpatialStatus::Conflicting, "{res:?}");
    assert!(res.conflicting().contains(&len));
    assert_at(&s, b, [10.0, 0.0, 0.0]);

    // Detached, B keeps two freedoms on a sphere around A.
    s.remove_constraint(at_b).unwrap();
    solve_ok(&mut s);
    assert_len(&s, l, 12.0);
    assert_at(&s, a, [0.0, 0.0, 0.0]);
    assert_at(&s, b, [12.0, 0.0, 0.0]);
    let an = s.analyze();
    assert_eq!((an.rank, an.dof), (4, 2));
    assert!(an.is_point_fully_constrained(a));
    assert!(!an.is_point_fully_constrained(b));
}

/// Component position `O + R p` of a local point (`r` row-major).
fn to_component(o: [f64; 3], r: [[f64; 3]; 3], p: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| o[i] + r[i][0] * p[0] + r[i][1] * p[1] + r[i][2] * p[2])
}

/// Local coordinates `R^T (v - O)` of a component position.
fn to_local(o: [f64; 3], r: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    let d = [v[0] - o[0], v[1] - o[1], v[2] - o[2]];
    [0, 1, 2].map(|j| r[0][j] * d[0] + r[1][j] * d[1] + r[2][j] * d[2])
}

#[test]
fn rotated_and_translated_frame_with_a_moving_source_vertex() {
    // 90 degrees about component Z: local x -> component y, local y ->
    // component -x.
    let o = [100.0, 200.0, 300.0];
    let rot = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let vertex = [100.0, 200.0, 300.0];
    let mut s = SpatialSystem::new();
    let r = route(&mut s, to_local(o, rot, vertex));
    solve_ok(&mut s);
    let comp = |s: &SpatialSystem, i: usize| to_component(o, rot, s.point(r.p[i]).unwrap());
    let want = [
        [100.0, 200.0, 300.0],
        [100.0, 240.0, 300.0],
        [70.0, 240.0, 300.0],
        [70.0, 240.0, 320.0],
    ];
    for (i, w) in want.iter().enumerate() {
        assert!(near(comp(&s, i), *w, EPS), "{i}: {:?}", comp(&s, i));
    }
    assert_eq!(s.analyze().rank, 12);

    s.set_value(r.len[1], 50.0).unwrap();
    solve_ok(&mut s);
    assert!(near(comp(&s, 2), [50.0, 240.0, 300.0], EPS));
    assert!(near(comp(&s, 3), [50.0, 240.0, 320.0], EPS));

    // The source vertex moves by (-3, 0, 5): the target moves by R^T dV
    // and the route follows it once.
    let moved = add(vertex, [-3.0, 0.0, 5.0]);
    let target = to_local(o, rot, moved);
    assert!(near(target, [0.0, 3.0, 5.0], 1e-12), "{target:?}");
    s.set_target(r.origin, target).unwrap();
    solve_ok(&mut s);
    assert!(near(comp(&s, 0), moved, EPS));
    assert!(near(comp(&s, 3), [47.0, 240.0, 325.0], EPS));
    for (l, len) in r.l.iter().zip([40.0, 50.0, 20.0]) {
        assert_len(&s, *l, len);
    }
    let a = s.analyze();
    assert_eq!((a.rank, a.dof), (12, 0));
}

#[test]
fn far_from_the_origin_the_tolerance_stays_absolute() {
    let at = [1.0e6, -2.0e6, 5.0e5];
    let mut s = SpatialSystem::new();
    let r = route(&mut s, at);
    let res = solve_ok(&mut s);
    assert!(res.residual <= 1e-6);
    let want = [
        [0.0, 0.0, 0.0],
        [40.0, 0.0, 0.0],
        [40.0, 30.0, 0.0],
        [40.0, 30.0, 20.0],
    ];
    for (p, w) in r.p.iter().zip(want) {
        assert_at(&s, *p, add(at, w));
    }
    assert!(s.residual() <= 1e-6);
    assert!(s.analyze().is_fully_constrained());
}

#[test]
fn a_long_route_keeps_the_absolute_tolerance() {
    let mut s = SpatialSystem::new();
    let axes = [Axis::X, Axis::Y, Axis::Z];
    let mut expected = vec![[0.0; 3]];
    let mut points = vec![s.add_point([0.4, -0.3, 0.2]).unwrap()];
    s.add_constraint(Attach {
        point: points[0],
        target: [0.0; 3],
    })
    .unwrap();
    for i in 0..60 {
        let axis = axes[i % 3];
        let sense = if (i / 3) % 2 == 0 { Positive } else { Negative };
        let len = 1000.0 + 7.0 * i as f64;
        let mut next = *expected.last().unwrap();
        next[axis.index()] += sense.sign() * len;
        let x = i as f64;
        let q = s
            .add_point(add(next, [x.sin(), (1.7 * x).cos(), 0.5]))
            .unwrap();
        let l = s.add_line(*points.last().unwrap(), q).unwrap();
        s.add_constraint(Align {
            line: l,
            axis,
            sense,
        })
        .unwrap();
        s.add_constraint(Length {
            line: l,
            value: len,
        })
        .unwrap();
        points.push(q);
        expected.push(next);
    }
    let res = solve_ok(&mut s);
    assert!(res.residual <= 1e-6);
    for (p, w) in points.iter().zip(&expected) {
        assert_at(&s, *p, *w);
    }
    let a = s.analyze();
    assert_eq!(a.dof, 0);
    assert!(a.is_fully_constrained());
}

#[test]
fn negative_sense_and_signed_offsets() {
    let mut s = SpatialSystem::new();
    let a = s.add_point([0.5, 0.2, -0.1]).unwrap();
    let b = s.add_point([-38.0, 1.0, 0.0]).unwrap();
    let c = s.add_point([-41.0, -28.0, 2.0]).unwrap();
    let ab = s.add_line(a, b).unwrap();
    s.add_constraint(Attach {
        point: a,
        target: [0.0; 3],
    })
    .unwrap();
    s.add_constraint(Align {
        line: ab,
        axis: Axis::X,
        sense: Negative,
    })
    .unwrap();
    s.add_constraint(Length {
        line: ab,
        value: 40.0,
    })
    .unwrap();
    // Signed offsets from B to C (not a line): zero, negative, zero.
    let offset = |s: &mut SpatialSystem, axis: Axis, value: f64| {
        s.add_constraint(Offset {
            a: b,
            b: c,
            axis,
            value,
        })
        .unwrap()
    };
    offset(&mut s, Axis::X, 0.0);
    let dy = offset(&mut s, Axis::Y, -30.0);
    offset(&mut s, Axis::Z, 0.0);
    solve_ok(&mut s);
    assert_at(&s, b, [-40.0, 0.0, 0.0]);
    assert_at(&s, c, [-40.0, -30.0, 0.0]);
    assert!((s.measure(dy).unwrap() + 30.0).abs() < EPS);
    let an = s.analyze();
    assert_eq!((an.unknowns, an.rank, an.dof), (9, 9, 0));
    assert!(an.is_fully_constrained());

    // An offset may change sign.
    s.set_value(dy, 12.0).unwrap();
    solve_ok(&mut s);
    assert_at(&s, c, [-40.0, 12.0, 0.0]);
    assert_at(&s, b, [-40.0, 0.0, 0.0]);
}

#[test]
fn an_aligned_line_keeps_its_sense() {
    // Without a length, the solve keeps the line on its axis; pointing
    // the other way is invalid, and nothing moves.
    let mut s = SpatialSystem::new();
    let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
    let b = s.add_point([-5.0, 1.0, 0.0]).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(Fix {
        point: a,
        at: [0.0; 3],
    })
    .unwrap();
    let align = s
        .add_constraint(Align {
            line: l,
            axis: Axis::X,
            sense: Positive,
        })
        .unwrap();
    let res = s.solve(&opts());
    assert_eq!(res.status, SpatialStatus::Invalid);
    assert_eq!(
        res.issues,
        vec![SpatialIssue::ReversedSense {
            constraint: align,
            line: l
        }]
    );
    assert_at(&s, b, [-5.0, 1.0, 0.0]);

    // An offset along the axis against the sense, or of zero, contradicts
    // the alignment before any solve.
    for value in [-5.0, 0.0] {
        let off = s
            .add_constraint(Offset {
                a,
                b,
                axis: Axis::X,
                value,
            })
            .unwrap();
        let res = s.solve(&opts());
        assert_eq!(res.status, SpatialStatus::Conflicting);
        assert_eq!(res.conflicting(), vec![align, off]);
        s.remove_constraint(off).unwrap();
    }
    // Seen from the other end (B to A) a negative offset agrees.
    s.add_constraint(Offset {
        a: b,
        b: a,
        axis: Axis::X,
        value: -7.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_at(&s, b, [7.0, 0.0, 0.0]);
}

#[test]
fn contradictory_axis_requests_are_conflicts() {
    for (second_axis, second_sense) in [(Axis::Y, Positive), (Axis::X, Negative)] {
        let mut s = SpatialSystem::new();
        let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
        let b = s.add_point([10.0, 1.0, 1.0]).unwrap();
        let l = s.add_line(a, b).unwrap();
        let first = s
            .add_constraint(Align {
                line: l,
                axis: Axis::X,
                sense: Positive,
            })
            .unwrap();
        let second = s
            .add_constraint(Align {
                line: l,
                axis: second_axis,
                sense: second_sense,
            })
            .unwrap();
        let res = s.solve(&opts());
        assert_eq!(res.status, SpatialStatus::Conflicting);
        assert_eq!(res.conflicting(), vec![first, second]);
        assert_eq!(res.conflicts[0].constraint, second);
        assert_at(&s, b, [10.0, 1.0, 1.0]);
        let an = s.analyze();
        assert_eq!(an.conflicting_constraints(), vec![first, second]);
        assert!(!an.is_fully_constrained());
    }
    // The same request twice is redundant, not a conflict.
    let mut s = SpatialSystem::new();
    let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
    let b = s.add_point([10.0, 1.0, 1.0]).unwrap();
    let l = s.add_line(a, b).unwrap();
    let align = Align {
        line: l,
        axis: Axis::Z,
        sense: Negative,
    };
    s.add_constraint(Fix {
        point: a,
        at: [0.0; 3],
    })
    .unwrap();
    s.add_constraint(align).unwrap();
    let twice = s.add_constraint(align).unwrap();
    let res = s.solve(&opts());
    // Pointing +Z by nothing: the solve lands on the axis at z = 1, the
    // wrong way for the negative sense.
    assert_eq!(res.status, SpatialStatus::Invalid);
    s.set_point(b, [0.0, 0.0, -10.0]).unwrap();
    solve_ok(&mut s);
    let an = s.analyze();
    assert_eq!(an.redundant_constraints(), vec![twice]);
    assert!(an.conflicts.is_empty());
}

#[test]
fn collapsed_lines_and_invalid_input_are_refused() {
    let mut s = SpatialSystem::new();
    let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
    let b = s.add_point([10.0, 0.0, 0.0]).unwrap();
    let l = s.add_line(a, b).unwrap();
    // A line from a point to itself, a relation of a point with itself.
    assert!(matches!(
        s.add_line(a, a),
        Err(SpatialError::InvalidValue(_))
    ));
    assert!(matches!(
        s.add_constraint(Coincident(a, a)),
        Err(SpatialError::InvalidValue(_))
    ));
    // Lengths at or below the shortest line, or not finite.
    for value in [
        0.0,
        -1.0,
        SPATIAL_MIN_SEGMENT,
        0.5e-6,
        f64::NAN,
        f64::INFINITY,
    ] {
        assert!(matches!(
            s.add_constraint(Length { line: l, value }),
            Err(SpatialError::InvalidValue(_))
        ));
    }
    assert!(matches!(
        s.add_point([0.0, f64::NAN, 0.0]),
        Err(SpatialError::InvalidValue(_))
    ));
    assert!(matches!(
        s.add_constraint(Fix {
            point: a,
            at: [f64::INFINITY, 0.0, 0.0]
        }),
        Err(SpatialError::InvalidValue(_))
    ));
    assert!(s.analyze().conflicts.is_empty());

    // Ends joined by a coincidence: collapsed before any solve.
    let c = s.add_constraint(Coincident(a, b)).unwrap();
    let res = s.solve(&opts());
    assert_eq!(res.status, SpatialStatus::Invalid);
    assert_eq!(res.issues, vec![SpatialIssue::CollapsedLine(l)]);
    assert_at(&s, b, [10.0, 0.0, 0.0]);
    assert!(!s.analyze().is_fully_constrained());
    s.remove_constraint(c).unwrap();

    // Ends solved closer than the shortest line.
    s.add_constraint(Fix {
        point: a,
        at: [0.0; 3],
    })
    .unwrap();
    s.add_constraint(Fix {
        point: b,
        at: [0.5e-6, 0.0, 0.0],
    })
    .unwrap();
    let res = s.solve(&opts());
    assert_eq!(res.status, SpatialStatus::Invalid);
    assert_eq!(res.issues, vec![SpatialIssue::CollapsedLine(l)]);
    assert_at(&s, b, [10.0, 0.0, 0.0]);
    // Full rank where nothing holds: not fully constrained.
    let an = s.analyze();
    assert_eq!((an.rank, an.dof), (6, 0));
    assert!(!an.solved && !an.is_fully_constrained());
    assert!(an.fully_constrained_points.is_empty());
    // Where everything holds, the collapsed line still keeps it from
    // being fully constrained.
    s.set_point(b, [0.5e-6, 0.0, 0.0]).unwrap();
    let an = s.analyze();
    assert_eq!((an.rank, an.dof), (6, 0));
    assert!(an.solved);
    assert!(an.issues.contains(&SpatialIssue::CollapsedLine(l)));
    assert!(!an.is_fully_constrained());
    assert!(an.fully_constrained_points.is_empty());
}

#[test]
fn degenerate_and_near_degenerate_lengths() {
    // Ends a nanometre apart: the length still has a direction.
    let mut s = SpatialSystem::new();
    let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
    let b = s.add_point([1e-9, 0.0, 0.0]).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(Fix {
        point: a,
        at: [0.0; 3],
    })
    .unwrap();
    s.add_constraint(Length {
        line: l,
        value: 10.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_at(&s, b, [10.0, 0.0, 0.0]);

    // Ends at one place: the length has no gradient there. Not a
    // conflict and not a solution: nothing moves.
    let mut s = SpatialSystem::new();
    let a = s.add_point([1.0, 2.0, 3.0]).unwrap();
    let b = s.add_point([1.0, 2.0, 3.0]).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(Fix {
        point: a,
        at: [1.0, 2.0, 3.0],
    })
    .unwrap();
    let len = s
        .add_constraint(Length {
            line: l,
            value: 10.0,
        })
        .unwrap();
    let res = s.solve(&opts());
    assert_eq!(res.status, SpatialStatus::NotConverged, "{res:?}");
    assert!(res.conflicts.is_empty());
    assert_eq!(res.issues, vec![SpatialIssue::Singular(len)]);
    assert_at(&s, b, [1.0, 2.0, 3.0]);
    let an = s.analyze();
    assert!(an.issues.contains(&SpatialIssue::Singular(len)));
    assert!(an.conflicts.is_empty());
    // Started a little apart, it solves.
    s.set_point(b, [1.0, 2.0, 4.0]).unwrap();
    solve_ok(&mut s);
    assert_at(&s, b, [1.0, 2.0, 13.0]);
}

#[test]
fn a_free_length_changes_along_the_line_and_an_alignment_recompiles_it() {
    let mut s = SpatialSystem::new();
    let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
    let b = s.add_point([3.0, 4.0, 12.0]).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(Fix {
        point: a,
        at: [0.0; 3],
    })
    .unwrap();
    let len = s
        .add_constraint(Length {
            line: l,
            value: 13.0,
        })
        .unwrap();
    solve_ok(&mut s);
    s.set_value(len, 26.0).unwrap();
    solve_ok(&mut s);
    // The smallest move keeps the direction.
    assert_at(&s, b, [6.0, 8.0, 24.0]);
    assert_eq!(s.analyze().dof, 2);

    // Aligned with Z, the same length becomes an axial difference.
    let align = s
        .add_constraint(Align {
            line: l,
            axis: Axis::Z,
            sense: Positive,
        })
        .unwrap();
    solve_ok(&mut s);
    assert_at(&s, b, [0.0, 0.0, 26.0]);
    let an = s.analyze();
    assert_eq!((an.rank, an.dof), (6, 0));
    assert!(an.is_fully_constrained());

    // Without the alignment the length is free to turn again.
    s.remove_constraint(align).unwrap();
    solve_ok(&mut s);
    assert_at(&s, b, [0.0, 0.0, 26.0]);
    assert_eq!(s.analyze().dof, 2);
}

#[test]
fn dragging_free_and_constrained_points() {
    // A wholly free point (no row mentions it) goes to the target.
    let mut s = SpatialSystem::new();
    let p = s.add_point([1.0, 1.0, 1.0]).unwrap();
    let res = drag(&mut s, p, [7.0, -8.0, 9.0]);
    assert!(res.is_ok());
    assert_at(&s, p, [7.0, -8.0, 9.0]);
    assert_eq!(s.analyze().dof, 3);

    // Partly free: an offset holds x, y and z follow the target.
    let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
    s.add_constraint(Fix {
        point: a,
        at: [0.0; 3],
    })
    .unwrap();
    s.add_constraint(Offset {
        a,
        b: p,
        axis: Axis::X,
        value: 5.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_at(&s, p, [5.0, -8.0, 9.0]);
    let res = drag(&mut s, p, [7.0, 3.0, -2.0]);
    assert!(res.is_ok());
    assert_at(&s, p, [5.0, 3.0, -2.0]);

    // A length from a fixed point: the end slides on the sphere toward
    // the pointer, the length holds.
    let mut s = SpatialSystem::new();
    let a = s.add_point([0.0, 0.0, 0.0]).unwrap();
    let b = s.add_point([10.0, 0.0, 0.0]).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(Fix {
        point: a,
        at: [0.0; 3],
    })
    .unwrap();
    s.add_constraint(Length {
        line: l,
        value: 10.0,
    })
    .unwrap();
    let res = drag(&mut s, b, [0.0, 20.0, 0.0]);
    assert!(res.is_ok(), "{res:?}");
    assert_len(&s, l, 10.0);
    assert_at(&s, a, [0.0, 0.0, 0.0]);
    let v = s.point(b).unwrap();
    assert!(near(v, [0.0, 10.0, 0.0], DRAG_EPS), "{v:?}");
    // Out of the plane as well.
    let res = drag(&mut s, b, [0.0, 0.0, -20.0]);
    assert!(res.is_ok(), "{res:?}");
    assert_len(&s, l, 10.0);
    let v = s.point(b).unwrap();
    assert!(near(v, [0.0, 0.0, -10.0], DRAG_EPS), "{v:?}");

    // Goals must name live points and finite targets.
    assert!(matches!(
        s.drag(
            &[SpatialDragGoal {
                point: b,
                target: [f64::NAN, 0.0, 0.0]
            }],
            &opts()
        ),
        Err(SpatialError::InvalidValue(_))
    ));
}

/// The planar system's components now come from the helper it shares
/// with the spatial system: implicit rows, free unknowns and fixed
/// geometry analyse as before.
#[test]
fn planar_components_keep_implicit_rows_and_free_unknowns() {
    use mitcad_solver::{Constraint, DragGoal, SolveOptions, System};
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let start = s.add_point(10.0, 0.0);
    let end = s.add_point(0.0, 10.0);
    let arc = s.add_arc(c, start, end).unwrap();
    let free = s.add_point(30.0, 30.0);
    let p = s.add_point(20.0, 0.0);
    let q = s.add_point(25.0, 5.0);
    s.add_line(p, q).unwrap();
    s.add_constraint(Constraint::FixPoint(p)).unwrap();
    assert!(s.solve(&SolveOptions::default()).is_ok());
    // The arc 5 (its equal-radius row), the free point 2, the line 2.
    assert_eq!(s.analyze().dof, 9);
    s.add_constraint(Constraint::FixEntity(arc)).unwrap();
    assert!(s.solve(&SolveOptions::default()).is_ok());
    let a = s.analyze();
    assert_eq!(a.dof, 4);
    assert!(a.is_entity_fully_constrained(arc));
    assert!(a.is_point_fully_constrained(p));
    assert!(!a.is_point_fully_constrained(free));
    let r = s.drag(
        &[DragGoal::Point {
            point: free,
            target: [1.0, 2.0],
        }],
        &SolveOptions::default(),
    );
    assert!(r.is_ok());
    let v = s.point(free).unwrap();
    assert!((v[0] - 1.0).abs() < EPS && (v[1] - 2.0).abs() < EPS);
    let v = s.point(start).unwrap();
    assert!((v[0] - 10.0).abs() < EPS && v[1].abs() < EPS);
}
