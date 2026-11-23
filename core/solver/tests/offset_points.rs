// SPDX-License-Identifier: MIT
//! Points held on the offset of a curve and on an offset's round corner
//! (mitcad#2278): `p = S(t) + left n(t)` with `t` free in a range, fixed or
//! at the curve's end, following the curve as it moves.

use mitcad_solver::Constraint::*;
use mitcad_solver::{OffsetAt, SolveOptions, System};

fn solve(s: &mut System) {
    let r = s.solve(&SolveOptions::default());
    assert!(r.is_ok(), "{r:?}");
}

/// The spline's point and unit left normal at `t` (central differences).
fn spline_frame(s: &System, e: mitcad_solver::EntityId, t: f64) -> ([f64; 2], [f64; 2]) {
    let h = 1e-6;
    let p = s.spline_point(e, t).unwrap();
    let (a, b) = (
        s.spline_point(e, (t - h).max(0.0)).unwrap(),
        s.spline_point(e, (t + h).min(1.0)).unwrap(),
    );
    let d = [b[0] - a[0], b[1] - a[1]];
    let n = (d[0] * d[0] + d[1] * d[1]).sqrt();
    (p, [-d[1] / n, d[0] / n])
}

/// The smallest distance from `p` to the spline (dense sampling, refined).
fn distance_to_spline(s: &System, e: mitcad_solver::EntityId, p: [f64; 2]) -> (f64, f64) {
    let d = |t: f64| {
        let q = s.spline_point(e, t).unwrap();
        ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt()
    };
    let mut best = (f64::INFINITY, 0.0);
    for k in 0..=20000 {
        let t = k as f64 / 20000.0;
        let v = d(t);
        if v < best.0 {
            best = (v, t);
        }
    }
    best
}

fn wavy(s: &mut System) -> (Vec<mitcad_solver::PointId>, mitcad_solver::EntityId) {
    let at = [
        [0.0, 0.0],
        [10.0, 8.0],
        [20.0, -4.0],
        [30.0, 6.0],
        [40.0, 0.0],
    ];
    let ctrl: Vec<_> = at.iter().map(|p| s.add_point(p[0], p[1])).collect();
    let e = s.add_bspline(3, &ctrl, None, None).unwrap();
    (ctrl, e)
}

#[test]
fn point_along_a_spline_offset_follows_the_spline() {
    let mut s = System::new();
    let (ctrl, e) = wavy(&mut s);
    for c in &ctrl {
        s.add_constraint(FixPoint(*c)).unwrap();
    }
    let p = s.add_point(18.0, 6.0);
    s.add_constraint(PointOnOffset {
        point: p,
        curve: e,
        left: 1.0,
        at: OffsetAt::Along {
            t: 0.45,
            range: Some((0.0, 1.0)),
        },
    })
    .unwrap();
    solve(&mut s);
    let at = s.point(p).unwrap();
    let (gap, _) = distance_to_spline(&s, e, at);
    assert!((gap - 1.0).abs() < 1e-5, "{gap}");
    // A point on the offset leaves the point one degree of freedom.
    assert_eq!(s.analyze().dof, 1);
    // The spline moves (a control point is set elsewhere): the point stays
    // on its offset.
    s.set_point(ctrl[2], [20.0, -9.0]).unwrap();
    solve(&mut s);
    let (gap, _) = distance_to_spline(&s, e, s.point(p).unwrap());
    assert!((gap - 1.0).abs() < 1e-5, "{gap}");
}

#[test]
fn point_at_a_spline_offset_end_is_exact() {
    let mut s = System::new();
    let (ctrl, e) = wavy(&mut s);
    for c in &ctrl {
        s.add_constraint(FixPoint(*c)).unwrap();
    }
    let q = s.add_point(41.0, 3.0);
    s.add_constraint(PointOnOffset {
        point: q,
        curve: e,
        left: -1.5,
        at: OffsetAt::End,
    })
    .unwrap();
    solve(&mut s);
    let (c, n) = spline_frame(&s, e, 1.0);
    let want = [c[0] - 1.5 * n[0], c[1] - 1.5 * n[1]];
    let at = s.point(q).unwrap();
    assert!((at[0] - want[0]).abs() < 1e-5 && (at[1] - want[1]).abs() < 1e-5);
    assert_eq!(s.analyze().dof, 0);
}

#[test]
fn point_along_a_range_stays_in_it() {
    let mut s = System::new();
    let (ctrl, e) = wavy(&mut s);
    for c in &ctrl {
        s.add_constraint(FixPoint(*c)).unwrap();
    }
    let p = s.add_point(5.0, 4.0);
    s.add_constraint(PointOnOffset {
        point: p,
        curve: e,
        left: 1.0,
        at: OffsetAt::Along {
            t: 0.5,
            range: Some((0.4, 0.6)),
        },
    })
    .unwrap();
    solve(&mut s);
    let at = s.point(p).unwrap();
    let (gap, t) = distance_to_spline(&s, e, at);
    assert!((gap - 1.0).abs() < 1e-4, "{gap}");
    assert!((0.4 - 1e-3..=0.6 + 1e-3).contains(&t), "{t}");
}

#[test]
fn point_on_line_arc_and_ellipse_offsets() {
    // A line: at 3 mm to its right.
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 0.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(FixPoint(a)).unwrap();
    s.add_constraint(FixPoint(b)).unwrap();
    let p = s.add_point(4.0, -1.0);
    s.add_constraint(PointOnOffset {
        point: p,
        curve: l,
        left: -3.0,
        at: OffsetAt::Along {
            t: 0.4,
            range: Some((0.0, 1.0)),
        },
    })
    .unwrap();
    solve(&mut s);
    assert!((s.point(p).unwrap()[1] + 3.0).abs() < 1e-9);

    // An arc of radius 5 offset 1 mm away from its centre (its right).
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let st = s.add_point(5.0, 0.0);
    let en = s.add_point(0.0, 5.0);
    let arc = s.add_arc(c, st, en).unwrap();
    for q in [c, st, en] {
        s.add_constraint(FixPoint(q)).unwrap();
    }
    let p = s.add_point(4.0, 4.0);
    s.add_constraint(PointOnOffset {
        point: p,
        curve: arc,
        left: -1.0,
        at: OffsetAt::Along {
            t: 0.7,
            range: Some((0.0, std::f64::consts::FRAC_PI_2)),
        },
    })
    .unwrap();
    let end = s.add_point(0.5, 5.5);
    s.add_constraint(PointOnOffset {
        point: end,
        curve: arc,
        left: -1.0,
        at: OffsetAt::End,
    })
    .unwrap();
    solve(&mut s);
    let q = s.point(p).unwrap();
    assert!(((q[0] * q[0] + q[1] * q[1]).sqrt() - 6.0).abs() < 1e-9);
    let q = s.point(end).unwrap();
    assert!(q[0].abs() < 1e-9 && (q[1] - 6.0).abs() < 1e-9, "{q:?}");

    // An ellipse arc's end, 1 mm outside it.
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let major = s.add_point(8.0, 0.0);
    let st = s.add_point(8.0, 0.0);
    let en = s.add_point(0.0, 4.0);
    let arc = s.add_elliptical_arc(c, major, 4.0, st, en).unwrap();
    for q in [c, major] {
        s.add_constraint(FixPoint(q)).unwrap();
    }
    s.add_constraint(FixEntity(arc)).unwrap();
    let p = s.add_point(0.3, 5.2);
    s.add_constraint(PointOnOffset {
        point: p,
        curve: arc,
        left: -1.0,
        at: OffsetAt::End,
    })
    .unwrap();
    solve(&mut s);
    let q = s.point(p).unwrap();
    assert!(q[0].abs() < 1e-7 && (q[1] - 5.0).abs() < 1e-7, "{q:?}");
}

#[test]
fn point_on_a_round_corner() {
    // Two lines meeting at (10, 0), turning left; the offset to the right
    // (outside) rounds the corner with radius 2 about it.
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let k = s.add_point(10.0, 0.0);
    let b = s.add_point(10.0, 10.0);
    let l1 = s.add_line(a, k).unwrap();
    let l2 = s.add_line(k, b).unwrap();
    for q in [a, k, b] {
        s.add_constraint(FixPoint(q)).unwrap();
    }
    let p = s.add_point(12.5, -2.5);
    s.add_constraint(PointOnOffsetCorner {
        point: p,
        corner: k,
        a: l1,
        a_at: OffsetAt::End,
        a_left: -2.0,
        b: l2,
        b_at: OffsetAt::Start,
        b_left: -2.0,
        s: 0.5,
    })
    .unwrap();
    solve(&mut s);
    let q = s.point(p).unwrap();
    let r = ((q[0] - 10.0).powi(2) + q[1].powi(2)).sqrt();
    assert!((r - 2.0).abs() < 1e-9, "{q:?}");
    // Within the arc's span: right of the corner and below it.
    assert!(q[0] >= 10.0 - 1e-9 && q[1] <= 1e-9, "{q:?}");
    assert_eq!(s.analyze().dof, 1);
}

#[test]
fn offsets_refuse_what_they_cannot_hold() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let circle = s.add_circle(c, 3.0).unwrap();
    let p = s.add_point(4.0, 0.0);
    assert!(
        s.add_constraint(PointOnOffset {
            point: p,
            curve: circle,
            left: 1.0,
            at: OffsetAt::End,
        })
        .is_err()
    );
    assert!(
        s.add_constraint(PointOnOffset {
            point: p,
            curve: circle,
            left: f64::NAN,
            at: OffsetAt::At(0.0),
        })
        .is_err()
    );
}
