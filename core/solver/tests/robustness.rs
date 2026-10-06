// SPDX-License-Identifier: MIT
//! Orientation is kept through dimension changes, and perturbed sketches
//! converge back (property-style tests with deterministic random numbers).

mod common;

use common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::{PointId, SolveOptions, System};
use std::f64::consts::PI;

fn signed_area(s: &System, pts: &[PointId]) -> f64 {
    let v: Vec<[f64; 2]> = pts.iter().map(|&p| pt(s, p)).collect();
    (0..v.len())
        .map(|i| {
            let (a, b) = (v[i], v[(i + 1) % v.len()]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum::<f64>()
        / 2.0
}

#[test]
fn triangle_apex_does_not_flip_when_sides_shrink() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 0.0);
    let c = s.add_point(5.0, 4.9);
    s.add_constraint(FixPoint(a)).unwrap();
    s.add_constraint(FixPoint(b)).unwrap();
    let da = s
        .add_constraint(Distance {
            a,
            b: c,
            value: 7.0,
        })
        .unwrap();
    let db = s
        .add_constraint(Distance {
            a: b,
            b: c,
            value: 7.0,
        })
        .unwrap();
    solve_ok(&mut s);
    for v in [5.2, 5.01, 5.0001, 6.0, 30.0, 5.000001] {
        s.set_dimension_value(da, v).unwrap();
        s.set_dimension_value(db, v).unwrap();
        solve_ok(&mut s);
        let y = pt(&s, c)[1];
        assert!(y > 0.0, "apex flipped below the base at {v}: {y}");
        assert_near(y, (v * v - 25.0).sqrt(), "apex height");
    }
}

#[test]
fn point_line_distance_keeps_the_side() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 0.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(FixEntity(l)).unwrap();
    let p = s.add_point(3.0, -2.0);
    let d = s
        .add_constraint(PointLineDistance {
            point: p,
            line: l,
            value: 2.0,
        })
        .unwrap();
    for v in [0.1, 0.0, 5.0, 1e-4, 50.0] {
        s.set_dimension_value(d, v).unwrap();
        solve_ok(&mut s);
        assert_near(pt(&s, p)[1], -v, "below the line");
    }
}

#[test]
fn rectangle_survives_large_dimension_changes() {
    let mut s = System::new();
    let r = rect(&mut s, 0.0, 0.0, 10.0, 5.0);
    s.add_constraint(FixPoint(r.p[0])).unwrap();
    let w = s
        .add_constraint(Length {
            line: r.l[0],
            value: 10.0,
        })
        .unwrap();
    let h = s
        .add_constraint(Length {
            line: r.l[1],
            value: 5.0,
        })
        .unwrap();
    let area0 = signed_area(&s, &r.p);
    for (wv, hv) in [(1000.0, 5.0), (0.5, 300.0), (0.01, 0.01), (42.0, 17.0)] {
        s.set_dimension_value(w, wv).unwrap();
        s.set_dimension_value(h, hv).unwrap();
        solve_ok(&mut s);
        assert_pt(&s, r.p[2], wv, hv);
        assert!(signed_area(&s, &r.p) * area0 > 0.0, "orientation kept");
    }
}

#[test]
fn large_angle_changes_rotate_the_same_way() {
    let mut s = System::new();
    let o = s.add_point(0.0, 0.0);
    let x = s.add_point(10.0, 0.0);
    let base = s.add_line(o, x).unwrap();
    s.add_constraint(FixEntity(base)).unwrap();
    let t = s.add_point(7.0, 7.0);
    let arm = s.add_line(o, t).unwrap();
    s.add_constraint(Length {
        line: arm,
        value: 10.0,
    })
    .unwrap();
    let ang = s
        .add_constraint(Angle {
            a: base,
            b: arm,
            value: PI / 4.0,
        })
        .unwrap();
    for v in [3.0, 0.01, 2.0, PI - 0.01, 0.5] {
        s.set_dimension_value(ang, v).unwrap();
        solve_ok(&mut s);
        assert_pt(&s, t, 10.0 * v.cos(), 10.0 * v.sin());
    }
}

#[test]
fn tangent_circle_keeps_its_side_when_the_radius_grows() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(20.0, 0.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(FixEntity(l)).unwrap();
    let c = s.add_point(10.0, 2.0);
    let circle = s.add_circle(c, 2.0).unwrap();
    s.add_constraint(Tangent(l, circle)).unwrap();
    s.add_constraint(HorizontalDistance {
        a,
        b: c,
        value: 10.0,
    })
    .unwrap();
    let r = s
        .add_constraint(Radius {
            entity: circle,
            value: 2.0,
        })
        .unwrap();
    for v in [0.1, 50.0, 3.0] {
        s.set_dimension_value(r, v).unwrap();
        solve_ok(&mut s);
        assert_pt(&s, c, 10.0, v);
    }
}

/// A fully constrained sketch: a slot with a tangent circle on top.
fn build_reference(s: &mut System) -> Vec<PointId> {
    let c1 = s.add_point(0.0, 0.0);
    let c2 = s.add_point(20.0, 0.0);
    let tl = s.add_point(0.0, 5.0);
    let tr = s.add_point(20.0, 5.0);
    let br = s.add_point(20.0, -5.0);
    let bl = s.add_point(0.0, -5.0);
    let left = s.add_arc(c1, tl, bl).unwrap();
    let right = s.add_arc(c2, br, tr).unwrap();
    let top = s.add_line(tr, tl).unwrap();
    let bottom = s.add_line(bl, br).unwrap();
    for (l, a) in [(top, left), (top, right), (bottom, left), (bottom, right)] {
        s.add_constraint(Tangent(l, a)).unwrap();
    }
    s.add_constraint(Equal(left, right)).unwrap();
    s.add_constraint(Radius {
        entity: left,
        value: 5.0,
    })
    .unwrap();
    s.add_constraint(Distance {
        a: c1,
        b: c2,
        value: 20.0,
    })
    .unwrap();
    s.add_constraint(FixPoint(c1)).unwrap();
    s.add_constraint(HorizontalPoints(c1, c2)).unwrap();
    let c3 = s.add_point(10.0, 9.0);
    let circle = s.add_circle(c3, 4.0).unwrap();
    s.add_constraint(Tangent(top, circle)).unwrap();
    s.add_constraint(Radius {
        entity: circle,
        value: 4.0,
    })
    .unwrap();
    s.add_constraint(HorizontalDistance {
        a: c1,
        b: c3,
        value: 10.0,
    })
    .unwrap();
    vec![c1, c2, tl, tr, br, bl, c3]
}

#[test]
fn random_perturbations_converge_back() {
    let mut reference = System::new();
    let pts = build_reference(&mut reference);
    solve_ok(&mut reference);
    assert_eq!(reference.analyze().dof, 0);
    let want: Vec<[f64; 2]> = pts.iter().map(|&p| pt(&reference, p)).collect();
    let mut rng = Rng(0x5eed);
    for round in 0..50 {
        let mut s = System::new();
        let pts = build_reference(&mut s);
        let amp = 0.5 + 2.0 * rng.next();
        for &p in &pts[1..] {
            let v = pt(&s, p);
            s.set_point(p, [v[0] + rng.sym(amp), v[1] + rng.sym(amp)])
                .unwrap();
        }
        let r = s.solve(&SolveOptions::default());
        assert!(r.is_ok(), "round {round} (amplitude {amp}): {r:?}");
        for (i, &p) in pts.iter().enumerate() {
            let v = pt(&s, p);
            assert!(
                dist(v, want[i]) < 1e-6,
                "round {round}: point {i} at {v:?}, want {:?}",
                want[i]
            );
        }
    }
}

#[test]
fn random_under_constrained_sketches_satisfy_all_constraints() {
    // Chains of lines with random lengths, angles and point-on-line
    // constraints from random starting geometry: every solve must satisfy
    // all constraints, and the geometry must not move far.
    let mut rng = Rng(42);
    for round in 0..30 {
        let mut s = System::new();
        let n = 8;
        let pts: Vec<_> = (0..=n)
            .map(|i| s.add_point(i as f64 * 10.0 + rng.sym(2.0), rng.sym(5.0)))
            .collect();
        let lines: Vec<_> = (0..n)
            .map(|i| s.add_line(pts[i], pts[i + 1]).unwrap())
            .collect();
        s.add_constraint(FixPoint(pts[0])).unwrap();
        let mut ids = Vec::new();
        for (i, &l) in lines.iter().enumerate() {
            match i % 3 {
                0 => ids.push(
                    s.add_constraint(Length {
                        line: l,
                        value: 8.0 + rng.next() * 4.0,
                    })
                    .unwrap(),
                ),
                1 => ids.push(
                    s.add_constraint(Angle {
                        a: lines[i - 1],
                        b: l,
                        value: 0.2 + rng.next() * 0.5,
                    })
                    .unwrap(),
                ),
                _ => ids.push(s.add_constraint(Equal(lines[i - 1], l)).unwrap()),
            }
        }
        let before: Vec<[f64; 2]> = pts.iter().map(|&p| pt(&s, p)).collect();
        let r = s.solve(&SolveOptions::default());
        assert!(r.is_ok(), "round {round}: {r:?}");
        assert!(s.residual() < 1e-7, "round {round}");
        for id in &ids {
            if let (Some(m), Some(v)) = (s.measure(*id), s.dimension_value(*id)) {
                assert!((m - v).abs() < 1e-6, "round {round}: {m} vs {v}");
            }
        }
        let moved = pts
            .iter()
            .zip(&before)
            .map(|(&p, b)| dist(pt(&s, p), *b))
            .fold(0.0, f64::max);
        assert!(moved < 60.0, "round {round}: moved {moved}");
        let an = s.analyze();
        assert!(
            an.conflicts.is_empty() && an.redundant.is_empty(),
            "round {round}: {an:?}"
        );
    }
}
