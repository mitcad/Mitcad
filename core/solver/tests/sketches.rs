// SPDX-License-Identifier: MIT
//! Classic sketches solved from imperfect geometry.

mod common;

use common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::{EntityId, PointId, SplineEnd, System};
use std::f64::consts::PI;

/// Traversal direction of an arc (counter-clockwise) at one of its points.
fn arc_tangent(s: &System, arc: EntityId, at: PointId) -> [f64; 2] {
    let c = pt(s, s.entity_points(arc).unwrap()[0]);
    let p = pt(s, at);
    let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
    let n = (dx * dx + dy * dy).sqrt();
    [-dy / n, dx / n]
}

fn unit(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let d = dist(a, b);
    [(b[0] - a[0]) / d, (b[1] - a[1]) / d]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

#[test]
fn fully_dimensioned_rectangle() {
    let mut s = System::new();
    let r = rect(&mut s, 0.0, 0.0, 40.0, 20.0);
    // Imperfect start: corners off by a few millimetres.
    for (i, d) in [[0.5, -0.3], [2.0, 1.5], [-1.0, 3.0], [0.7, -2.0]]
        .iter()
        .enumerate()
    {
        let v = pt(&s, r.p[i]);
        s.set_point(r.p[i], [v[0] + d[0], v[1] + d[1]]).unwrap();
    }
    s.add_constraint(FixPoint(r.p[0])).unwrap();
    s.add_constraint(HorizontalDistance {
        a: r.p[0],
        b: r.p[1],
        value: 40.0,
    })
    .unwrap();
    let h = s
        .add_constraint(VerticalDistance {
            a: r.p[1],
            b: r.p[2],
            value: 20.0,
        })
        .unwrap();
    solve_ok(&mut s);
    let o = pt(&s, r.p[0]);
    assert_eq!(o, [0.5, -0.3], "fixed corner stays");
    assert_pt(&s, r.p[2], 40.5, 19.7);
    assert_eq!(s.analyze().dof, 0);
    s.set_dimension_value(h, 35.0).unwrap();
    let res = solve_ok(&mut s);
    assert_pt(&s, r.p[2], 40.5, 34.7);
    assert_pt(&s, r.p[3], 0.5, 34.7);
    assert!(res.iterations <= 10, "{res:?}");
}

#[test]
fn slot_from_two_arcs_and_two_tangent_lines() {
    let mut s = System::new();
    let c1 = s.add_point(0.3, 0.2);
    let c2 = s.add_point(19.0, -0.5);
    let tl = s.add_point(0.5, 5.5);
    let tr = s.add_point(20.5, 4.5);
    let br = s.add_point(19.5, -5.5);
    let bl = s.add_point(-0.5, -4.5);
    let left = s.add_arc(c1, tl, bl).unwrap();
    let right = s.add_arc(c2, br, tr).unwrap();
    let top = s.add_line(tr, tl).unwrap();
    let bottom = s.add_line(bl, br).unwrap();
    for (l, a) in [(top, left), (top, right), (bottom, left), (bottom, right)] {
        s.add_constraint(Tangent(l, a)).unwrap();
    }
    s.add_constraint(Equal(left, right)).unwrap();
    let r = s
        .add_constraint(Radius {
            entity: left,
            value: 5.0,
        })
        .unwrap();
    let len = s
        .add_constraint(Distance {
            a: c1,
            b: c2,
            value: 20.0,
        })
        .unwrap();
    s.add_constraint(FixPoint(c1)).unwrap();
    s.add_constraint(HorizontalPoints(c1, c2)).unwrap();
    solve_ok(&mut s);
    let o = pt(&s, c1);
    let check = |s: &System, r: f64, l: f64| {
        assert_pt(s, c2, o[0] + l, o[1]);
        assert_pt(s, tl, o[0], o[1] + r);
        assert_pt(s, tr, o[0] + l, o[1] + r);
        assert_pt(s, br, o[0] + l, o[1] - r);
        assert_pt(s, bl, o[0], o[1] - r);
    };
    check(&s, 5.0, 20.0);
    let an = s.analyze();
    assert_eq!(an.dof, 0, "{an:?}");
    assert!(an.redundant.is_empty() && an.conflicts.is_empty());
    assert_eq!(an.fully_constrained_entities.len(), 4);
    s.set_dimension_value(r, 8.0).unwrap();
    s.set_dimension_value(len, 30.0).unwrap();
    solve_ok(&mut s);
    check(&s, 8.0, 30.0);
}

#[test]
fn hexagon_with_equal_sides_on_a_circle() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let circle = s.add_circle(c, 9.0).unwrap();
    let mut rng = Rng(7);
    let p: Vec<_> = (0..6)
        .map(|i| {
            let a = i as f64 * PI / 3.0 + rng.sym(0.1);
            s.add_point(9.5 * a.cos(), 9.5 * a.sin())
        })
        .collect();
    let l: Vec<_> = (0..6)
        .map(|i| s.add_line(p[i], p[(i + 1) % 6]).unwrap())
        .collect();
    s.add_constraint(FixPoint(c)).unwrap();
    s.add_constraint(Radius {
        entity: circle,
        value: 10.0,
    })
    .unwrap();
    for &q in &p {
        s.add_constraint(PointOnCurve(q, circle)).unwrap();
    }
    for i in 1..6 {
        s.add_constraint(Equal(l[0], l[i])).unwrap();
    }
    s.add_constraint(Horizontal(l[1])).unwrap();
    solve_ok(&mut s);
    for &side in &l {
        assert_near(line_len(&s, side), 10.0, "regular side");
    }
    let an = s.analyze();
    assert_eq!(an.dof, 0, "{an:?}");
    assert!(an.redundant.is_empty());
}

#[test]
fn symmetric_profile_about_a_centreline() {
    let mut s = System::new();
    let axis_a = s.add_point(0.0, -5.0);
    let axis_b = s.add_point(0.0, 40.0);
    let axis = s.add_line(axis_a, axis_b).unwrap();
    s.add_constraint(FixEntity(axis)).unwrap();
    // Left half and an imperfect right half.
    let l0 = s.add_point(-10.0, 0.0);
    let l1 = s.add_point(-10.5, 20.0);
    let l2 = s.add_point(-4.0, 30.0);
    let r0 = s.add_point(11.0, 0.5);
    let r1 = s.add_point(9.5, 19.0);
    let r2 = s.add_point(4.5, 31.0);
    let pts = [l0, l1, l2, r2, r1, r0];
    let lines: Vec<_> = (0..6)
        .map(|i| s.add_line(pts[i], pts[(i + 1) % 6]).unwrap())
        .collect();
    for (a, b) in [(l0, r0), (l1, r1), (l2, r2)] {
        s.add_constraint(SymmetricPoints { a, b, axis }).unwrap();
    }
    s.add_constraint(PointOnCurve(axis_a, lines[5])).unwrap();
    s.add_constraint(Vertical(lines[0])).unwrap();
    s.add_constraint(Distance {
        a: l0,
        b: r0,
        value: 20.0,
    })
    .unwrap();
    s.add_constraint(Length {
        line: lines[0],
        value: 20.0,
    })
    .unwrap();
    s.add_constraint(Length {
        line: lines[2],
        value: 8.0,
    })
    .unwrap();
    s.add_constraint(VerticalDistance {
        a: l0,
        b: l2,
        value: 30.0,
    })
    .unwrap();
    solve_ok(&mut s);
    for (a, b) in [(l0, r0), (l1, r1), (l2, r2)] {
        let (pa, pb) = (pt(&s, a), pt(&s, b));
        assert_near(pa[0], -pb[0], "mirrored");
        assert_near(pa[1], pb[1], "mirrored");
    }
    assert_pt(&s, l0, -10.0, -5.0);
    assert_pt(&s, l1, -10.0, 15.0);
    assert_pt(&s, l2, -4.0, 25.0);
    assert_eq!(s.analyze().dof, 0);
}

#[test]
fn tangent_arc_chain_is_smooth() {
    let mut s = System::new();
    let p0 = s.add_point(-10.0, 0.0);
    let p1 = s.add_point(0.0, 0.0);
    let line = s.add_line(p0, p1).unwrap();
    s.add_constraint(FixEntity(line)).unwrap();
    // Arc 1 turns left from the line's end, arc 2 turns right (S curve),
    // then a line leaves to the right. Start a bit off.
    let c1 = s.add_point(0.4, 4.6);
    let j = s.add_point(5.3, 5.2);
    let a1 = s.add_arc(c1, p1, j).unwrap();
    let c2 = s.add_point(10.4, 5.5);
    let s2 = s.add_point(10.2, 10.3);
    let a2 = s.add_arc(c2, s2, j).unwrap();
    let q = s.add_point(19.0, 10.5);
    let out = s.add_line(s2, q).unwrap();
    s.add_constraint(Tangent(line, a1)).unwrap();
    s.add_constraint(Tangent(a1, a2)).unwrap();
    s.add_constraint(Tangent(a2, out)).unwrap();
    s.add_constraint(Radius {
        entity: a1,
        value: 5.0,
    })
    .unwrap();
    s.add_constraint(Equal(a1, a2)).unwrap();
    s.add_constraint(Horizontal(out)).unwrap();
    s.add_constraint(Length {
        line: out,
        value: 10.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_eq!(s.analyze().dof, 1, "the sweep of the first arc is free");
    let v = s
        .add_constraint(VerticalDistance {
            a: p1,
            b: q,
            value: 10.0,
        })
        .unwrap();
    solve_ok(&mut s);
    assert_eq!(s.analyze().dof, 0);
    assert_pt(&s, c1, 0.0, 5.0);
    assert_pt(&s, j, 5.0, 5.0);
    assert_pt(&s, c2, 10.0, 5.0);
    assert_pt(&s, s2, 10.0, 10.0);
    assert_pt(&s, q, 20.0, 10.0);
    // G1 at every joint: traversal directions agree (no cusps).
    let check_smooth = |s: &System| {
        let d_line = unit(pt(s, p0), pt(s, p1));
        let t1 = arc_tangent(s, a1, p1);
        assert!(dot(d_line, t1) > 0.999_999, "line -> arc 1");
        // Arc 1 is traversed forward, arc 2 backward (from its end).
        let t1e = arc_tangent(s, a1, j);
        let t2 = arc_tangent(s, a2, j);
        assert!(dot(t1e, [-t2[0], -t2[1]]) > 0.999_999, "arc 1 -> arc 2");
        let t2s = arc_tangent(s, a2, s2);
        let d_out = unit(pt(s, s2), pt(s, q));
        assert!(dot([-t2s[0], -t2s[1]], d_out) > 0.999_999, "arc 2 -> line");
        assert!(cross(d_line, d_out).abs() < 1e-9);
    };
    check_smooth(&s);
    // A deeper S keeps the joints smooth.
    s.set_dimension_value(v, 14.0).unwrap();
    solve_ok(&mut s);
    check_smooth(&s);
    assert_near(pt(&s, q)[1], 14.0, "new height");
}

#[test]
fn concentric_circles() {
    let mut s = System::new();
    let c = s.add_point(1.0, 1.0);
    let outer = s.add_circle(c, 9.0).unwrap();
    let c2 = s.add_point(1.5, 0.5);
    let inner = s.add_circle(c2, 4.0).unwrap();
    s.add_constraint(Concentric(outer, inner)).unwrap();
    s.add_constraint(FixPoint(c)).unwrap();
    s.add_constraint(Diameter {
        entity: outer,
        value: 20.0,
    })
    .unwrap();
    s.add_constraint(Radius {
        entity: inner,
        value: 5.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_pt(&s, c2, 1.0, 1.0);
    assert_near(s.radius(outer).unwrap(), 10.0, "outer");
    assert_near(s.radius(inner).unwrap(), 5.0, "inner");
    let an = s.analyze();
    assert_eq!(an.dof, 0);
    assert_eq!(an.fully_constrained_entities, vec![outer, inner]);
}

#[test]
fn spline_with_fixed_end_tangents() {
    let mut s = System::new();
    let a0 = s.add_point(-10.0, 0.0);
    let a1 = s.add_point(0.0, 0.0);
    let left = s.add_line(a0, a1).unwrap();
    s.add_constraint(FixEntity(left)).unwrap();
    let b0 = s.add_point(20.0, 10.0);
    let b1 = s.add_point(30.0, 10.0);
    let right = s.add_line(b0, b1).unwrap();
    s.add_constraint(Horizontal(right)).unwrap();
    s.add_constraint(FixPoint(b1)).unwrap();
    let len = s
        .add_constraint(Length {
            line: right,
            value: 10.0,
        })
        .unwrap();
    let ctrl = [
        a1,
        s.add_point(5.0, 1.0),
        s.add_point(10.0, 6.0),
        s.add_point(15.0, 9.0),
        b0,
    ];
    let sp = s.add_bspline(3, &ctrl, None, None).unwrap();
    s.add_constraint(Tangent(left, sp)).unwrap();
    s.add_constraint(Tangent(sp, right)).unwrap();
    let check = |s: &System| {
        let g = s.spline(sp).unwrap();
        let c = &g.control_points;
        assert_near(c[1][1], c[0][1], "start tangent horizontal");
        assert_near(c[3][1], c[4][1], "end tangent horizontal");
        assert!(
            c[1][0] > c[0][0] && c[3][0] < c[4][0],
            "legs point into the curve"
        );
    };
    solve_ok(&mut s);
    check(&s);
    s.set_dimension_value(len, 4.0).unwrap();
    solve_ok(&mut s);
    check(&s);
    assert_pt(&s, b0, 26.0, 10.0);
    // Moving an interior control point keeps the end tangents.
    s.set_point(ctrl[2], [12.0, -3.0]).unwrap();
    solve_ok(&mut s);
    check(&s);
}

#[test]
fn fitted_spline_tangent_to_a_line() {
    let mut s = System::new();
    let a = s.add_point(-10.0, 0.0);
    let f: Vec<_> = [[0.0, 0.0], [10.0, 6.0], [20.0, 2.0], [30.0, 8.0]]
        .iter()
        .map(|c| s.add_point(c[0], c[1]))
        .collect();
    let line = s.add_line(a, f[0]).unwrap();
    s.add_constraint(FixEntity(line)).unwrap();
    let sp = s.add_fitted_spline(&f, [SplineEnd::Natural; 2]).unwrap();
    assert_eq!(
        s.analyze().dof,
        6,
        "fit points (the first is fixed by the line)"
    );
    s.add_constraint(Tangent(line, sp)).unwrap();
    solve_ok(&mut s);
    let g = s.spline(sp).unwrap();
    assert_near(g.control_points[1][1], 0.0, "start tangent along the line");
    // The natural end condition gave way to the tangent: one more freedom
    // (the tangent magnitude) minus the tangent direction.
    assert_eq!(s.analyze().dof, 7);
    for (i, &p) in f.iter().enumerate().skip(1).take(2) {
        let v = pt(&s, p);
        let best = (0..=4000)
            .map(|k| dist(s.spline_point(sp, k as f64 / 4000.0).unwrap(), v))
            .fold(f64::INFINITY, f64::min);
        assert!(best < 1e-2, "fit point {i} on the curve");
    }
}

#[test]
fn triangle_with_angles_and_lengths() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 0.5);
    let c = s.add_point(4.0, 7.0);
    let ab = s.add_line(a, b).unwrap();
    let bc = s.add_line(b, c).unwrap();
    let ca = s.add_line(c, a).unwrap();
    s.add_constraint(FixPoint(a)).unwrap();
    s.add_constraint(Horizontal(ab)).unwrap();
    s.add_constraint(Length {
        line: ab,
        value: 10.0,
    })
    .unwrap();
    s.add_constraint(Angle {
        a: ab,
        b: ca,
        value: PI / 3.0,
    })
    .unwrap();
    s.add_constraint(Angle {
        a: ab,
        b: bc,
        value: PI / 3.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_pt(&s, c, 5.0, 5.0 * 3f64.sqrt());
    let an = s.analyze();
    assert_eq!(an.dof, 0, "{an:?}");
    assert!(an.redundant.is_empty());
    assert_near(line_len(&s, bc), 10.0, "equilateral");
    assert_near(line_len(&s, ca), 10.0, "equilateral");
}
