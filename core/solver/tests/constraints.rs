// SPDX-License-Identifier: MIT
//! Each constraint and dimension alone (and in small combinations).

mod common;

use common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::{EntityKind, Error, SplineEnd, System};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

#[test]
fn coincident_moves_both_points_halfway() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(2.0, 4.0);
    s.add_constraint(Coincident(a, b)).unwrap();
    solve_ok(&mut s);
    // Minimal movement: both move to the middle.
    assert_pt(&s, a, 1.0, 2.0);
    assert_pt(&s, b, 1.0, 2.0);
}

#[test]
fn fixed_point_does_not_move() {
    let mut s = System::new();
    let a = s.add_point(1.0, 1.0);
    let b = s.add_point(5.0, 3.0);
    s.add_constraint(FixPoint(a)).unwrap();
    s.add_constraint(Coincident(a, b)).unwrap();
    solve_ok(&mut s);
    assert_pt(&s, a, 1.0, 1.0);
    assert_pt(&s, b, 1.0, 1.0);
}

#[test]
fn fix_entity_holds_line_and_circle() {
    let mut s = System::new();
    let p = s.add_point(0.0, 0.0);
    let q = s.add_point(10.0, 0.0);
    let l = s.add_line(p, q).unwrap();
    let c = s.add_point(5.0, 7.0);
    let circle = s.add_circle(c, 3.0).unwrap();
    s.add_constraint(FixEntity(l)).unwrap();
    s.add_constraint(FixEntity(circle)).unwrap();
    s.add_constraint(Length {
        line: l,
        value: 20.0,
    })
    .unwrap();
    let r = s.solve(&opts());
    assert!(!r.is_ok(), "a fixed line cannot change length");
    assert_pt(&s, q, 10.0, 0.0);
    assert_near(s.radius(circle).unwrap(), 3.0, "radius");
}

#[test]
fn point_on_line_projects_perpendicularly() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 0.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(FixEntity(l)).unwrap();
    let p = s.add_point(4.0, 3.0);
    s.add_constraint(PointOnCurve(p, l)).unwrap();
    solve_ok(&mut s);
    assert_pt(&s, p, 4.0, 0.0);
}

#[test]
fn point_on_line_beyond_its_end() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 0.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(FixEntity(l)).unwrap();
    let p = s.add_point(15.0, 2.0);
    s.add_constraint(PointOnCurve(p, l)).unwrap();
    solve_ok(&mut s);
    assert_pt(&s, p, 15.0, 0.0);
}

#[test]
fn point_on_circle_and_arc() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let circle = s.add_circle(c, 5.0).unwrap();
    s.add_constraint(FixEntity(circle)).unwrap();
    let p = s.add_point(6.0, 8.0);
    s.add_constraint(PointOnCurve(p, circle)).unwrap();
    let c2 = s.add_point(20.0, 0.0);
    let st = s.add_point(24.0, 0.0);
    let en = s.add_point(20.0, 4.0);
    let arc = s.add_arc(c2, st, en).unwrap();
    s.add_constraint(FixEntity(arc)).unwrap();
    let q = s.add_point(23.0, 3.0);
    s.add_constraint(PointOnCurve(q, arc)).unwrap();
    solve_ok(&mut s);
    assert_pt(&s, p, 3.0, 4.0);
    assert_near(dist(pt(&s, q), [20.0, 0.0]), 4.0, "on arc");
}

#[test]
fn point_on_ellipse() {
    let mut s = System::new();
    let c = s.add_point(1.0, 2.0);
    let a = s.add_point(11.0, 2.0);
    let e = s.add_ellipse(c, a, 4.0).unwrap();
    s.add_constraint(FixEntity(e)).unwrap();
    let p = s.add_point(4.0, 9.0);
    s.add_constraint(PointOnCurve(p, e)).unwrap();
    solve_ok(&mut s);
    let v = pt(&s, p);
    let (x, y) = ((v[0] - 1.0) / 10.0, (v[1] - 2.0) / 4.0);
    assert_near(x * x + y * y, 1.0, "on ellipse");
    assert!(v[1] > 2.0, "stays on the near side");
}

#[test]
fn point_on_spline() {
    let mut s = System::new();
    let ctrl: Vec<_> = [[0.0, 0.0], [5.0, 10.0], [10.0, -5.0], [15.0, 5.0]]
        .iter()
        .map(|c| s.add_point(c[0], c[1]))
        .collect();
    let sp = s.add_bspline(3, &ctrl, None, None).unwrap();
    s.add_constraint(FixEntity(sp)).unwrap();
    let p = s.add_point(7.0, 6.0);
    s.add_constraint(PointOnCurve(p, sp)).unwrap();
    solve_ok(&mut s);
    // The point lies on the curve: compare with a dense sampling.
    let v = pt(&s, p);
    let best = (0..=2000)
        .map(|i| dist(s.spline_point(sp, i as f64 / 2000.0).unwrap(), v))
        .fold(f64::INFINITY, f64::min);
    assert!(best < 1e-2, "distance to curve {best}");
    assert!(dist(v, [7.0, 6.0]) < 3.0, "moved to a nearby point");
}

#[test]
fn horizontal_and_vertical_lines_and_points() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 1.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(Horizontal(l)).unwrap();
    let c = s.add_point(20.0, 0.0);
    let d = s.add_point(21.0, 8.0);
    let m = s.add_line(c, d).unwrap();
    s.add_constraint(Vertical(m)).unwrap();
    let e = s.add_point(30.0, 0.0);
    let f = s.add_point(40.0, -2.0);
    s.add_constraint(HorizontalPoints(e, f)).unwrap();
    let g = s.add_point(50.0, 0.0);
    let h = s.add_point(52.0, 5.0);
    s.add_constraint(VerticalPoints(g, h)).unwrap();
    solve_ok(&mut s);
    let (p, q) = ends(&s, l);
    assert_near(p[1], q[1], "horizontal");
    assert_near(p[1], 0.5, "minimal movement");
    let (p, q) = ends(&s, m);
    assert_near(p[0], q[0], "vertical");
    assert_near(pt(&s, e)[1], pt(&s, f)[1], "horizontal points");
    assert_near(pt(&s, g)[0], pt(&s, h)[0], "vertical points");
}

fn two_lines(
    s: &mut System,
    a: [f64; 4],
    b: [f64; 4],
) -> (mitcad_solver::EntityId, mitcad_solver::EntityId) {
    let p = [s.add_point(a[0], a[1]), s.add_point(a[2], a[3])];
    let q = [s.add_point(b[0], b[1]), s.add_point(b[2], b[3])];
    (
        s.add_line(p[0], p[1]).unwrap(),
        s.add_line(q[0], q[1]).unwrap(),
    )
}

#[test]
fn parallel_and_perpendicular() {
    let mut s = System::new();
    let (a, b) = two_lines(&mut s, [0.0, 0.0, 10.0, 1.0], [0.0, 5.0, 10.0, 4.0]);
    s.add_constraint(Parallel(a, b)).unwrap();
    let (c, d) = two_lines(&mut s, [20.0, 0.0, 30.0, 1.0], [25.0, -3.0, 26.0, 6.0]);
    s.add_constraint(Perpendicular(c, d)).unwrap();
    solve_ok(&mut s);
    let da = line_angle(&s, a);
    let db = line_angle(&s, b);
    assert!((da - db).abs() < EPS || ((da - db).abs() - PI).abs() < EPS);
    let dc = line_angle(&s, c);
    let dd = line_angle(&s, d);
    assert_near(
        ((dc - dd).abs() % PI - FRAC_PI_2).abs(),
        0.0,
        "perpendicular",
    );
}

#[test]
fn collinear_lines() {
    let mut s = System::new();
    let (a, b) = two_lines(&mut s, [0.0, 0.0, 10.0, 0.0], [12.0, 1.0, 20.0, -1.0]);
    s.add_constraint(FixEntity(a)).unwrap();
    s.add_constraint(Collinear(a, b)).unwrap();
    solve_ok(&mut s);
    let (p, q) = ends(&s, b);
    assert_near(p[1], 0.0, "on line");
    assert_near(q[1], 0.0, "on line");
}

#[test]
fn collinear_lines_sharing_a_point() {
    let mut s = System::new();
    let p = s.add_point(0.0, 0.0);
    let q = s.add_point(10.0, 0.0);
    let r = s.add_point(20.0, 3.0);
    let a = s.add_line(p, q).unwrap();
    let b = s.add_line(q, r).unwrap();
    s.add_constraint(FixEntity(a)).unwrap();
    let id = s.add_constraint(Collinear(a, b)).unwrap();
    solve_ok(&mut s);
    assert_pt(&s, r, 20.0, 0.0);
    let an = s.analyze();
    assert!(an.redundant.is_empty(), "{an:?} ({id:?})");
    assert_eq!(an.dof, 1);
}

#[test]
fn equal_lengths_and_radii() {
    let mut s = System::new();
    let (a, b) = two_lines(&mut s, [0.0, 0.0, 10.0, 0.0], [0.0, 5.0, 4.0, 5.0]);
    s.add_constraint(FixEntity(a)).unwrap();
    s.add_constraint(Equal(a, b)).unwrap();
    let c1 = s.add_point(30.0, 0.0);
    let circle = s.add_circle(c1, 5.0).unwrap();
    s.add_constraint(FixEntity(circle)).unwrap();
    let c2 = s.add_point(50.0, 0.0);
    let st = s.add_point(53.0, 0.0);
    let en = s.add_point(50.0, 3.0);
    let arc = s.add_arc(c2, st, en).unwrap();
    s.add_constraint(Equal(circle, arc)).unwrap();
    solve_ok(&mut s);
    assert_near(line_len(&s, b), 10.0, "equal length");
    assert_near(s.radius(arc).unwrap(), 5.0, "equal radius");
    assert_eq!(
        s.add_constraint(Equal(a, circle)),
        Err(Error::Unsupported(
            "equal needs two lines or two circles/arcs"
        ))
    );
}

#[test]
fn midpoint_of_line_and_arc() {
    let mut s = System::new();
    let (l, _) = two_lines(&mut s, [0.0, 0.0, 10.0, 4.0], [0.0, 1.0, 1.0, 1.0]);
    s.add_constraint(FixEntity(l)).unwrap();
    let m = s.add_point(3.0, 3.0);
    s.add_constraint(Midpoint(m, l)).unwrap();
    // Arc from 0 to 270 degrees: the midpoint is at 135 degrees.
    let c = s.add_point(20.0, 0.0);
    let st = s.add_point(25.0, 0.0);
    let en = s.add_point(20.0, -5.0);
    let arc = s.add_arc(c, st, en).unwrap();
    s.add_constraint(FixEntity(arc)).unwrap();
    let am = s.add_point(16.0, 4.0);
    s.add_constraint(Midpoint(am, arc)).unwrap();
    solve_ok(&mut s);
    assert_pt(&s, m, 5.0, 2.0);
    let h = 5.0 * FRAC_PI_4.cos();
    assert_pt(&s, am, 20.0 - h, h);
}

#[test]
fn concentric_circles_and_arc() {
    let mut s = System::new();
    let c1 = s.add_point(0.0, 0.0);
    let a = s.add_circle(c1, 5.0).unwrap();
    s.add_constraint(FixPoint(c1)).unwrap();
    let c2 = s.add_point(1.0, 1.0);
    let b = s.add_circle(c2, 8.0).unwrap();
    let c3 = s.add_point(-1.0, 0.5);
    let st = s.add_point(2.0, 0.5);
    let en = s.add_point(-1.0, 3.5);
    let arc = s.add_arc(c3, st, en).unwrap();
    s.add_constraint(Concentric(a, b)).unwrap();
    s.add_constraint(Concentric(a, arc)).unwrap();
    solve_ok(&mut s);
    assert_pt(&s, c2, 0.0, 0.0);
    assert_pt(&s, c3, 0.0, 0.0);
    assert_near(
        dist(pt(&s, st), [0.0, 0.0]),
        dist(pt(&s, en), [0.0, 0.0]),
        "arc radius",
    );
}

#[test]
fn symmetric_points_about_a_line() {
    let mut s = System::new();
    let (axis, _) = two_lines(&mut s, [0.0, 0.0, 0.0, 10.0], [5.0, 5.0, 6.0, 6.0]);
    s.add_constraint(FixEntity(axis)).unwrap();
    let a = s.add_point(-3.0, 2.0);
    let b = s.add_point(4.0, 3.0);
    s.add_constraint(SymmetricPoints { a, b, axis }).unwrap();
    solve_ok(&mut s);
    let (pa, pb) = (pt(&s, a), pt(&s, b));
    assert_near(pa[0], -pb[0], "mirrored x");
    assert_near(pa[1], pb[1], "same y");
}

#[test]
fn symmetric_lines_circles_and_arcs() {
    let mut s = System::new();
    let (axis, _) = two_lines(&mut s, [0.0, -10.0, 0.0, 10.0], [50.0, 50.0, 51.0, 51.0]);
    s.add_constraint(FixEntity(axis)).unwrap();
    let (la, lb) = two_lines(&mut s, [-5.0, 0.0, -2.0, 4.0], [5.5, 0.2, 2.0, 3.5]);
    s.add_constraint(FixEntity(la)).unwrap();
    s.add_constraint(SymmetricEntities { a: la, b: lb, axis })
        .unwrap();
    let ca = s.add_point(-6.0, 6.0);
    let circ_a = s.add_circle(ca, 2.0).unwrap();
    s.add_constraint(FixEntity(circ_a)).unwrap();
    let cb = s.add_point(5.0, 6.5);
    let circ_b = s.add_circle(cb, 1.5).unwrap();
    s.add_constraint(SymmetricEntities {
        a: circ_a,
        b: circ_b,
        axis,
    })
    .unwrap();
    // Arc a: centre (-5, -5), from 0 to 90 degrees, radius 2.
    let ac = s.add_point(-5.0, -5.0);
    let as_ = s.add_point(-3.0, -5.0);
    let ae = s.add_point(-5.0, -3.0);
    let arc_a = s.add_arc(ac, as_, ae).unwrap();
    s.add_constraint(FixEntity(arc_a)).unwrap();
    let bc = s.add_point(5.2, -5.1);
    let bs = s.add_point(5.0, -3.2);
    let be = s.add_point(3.1, -5.0);
    let arc_b = s.add_arc(bc, bs, be).unwrap();
    s.add_constraint(SymmetricEntities {
        a: arc_a,
        b: arc_b,
        axis,
    })
    .unwrap();
    solve_ok(&mut s);
    let (p, q) = ends(&s, lb);
    assert_near(dist(p, [5.0, 0.0]), 0.0, "line end 1");
    assert_near(dist(q, [2.0, 4.0]), 0.0, "line end 2");
    assert_pt(&s, cb, 6.0, 6.0);
    assert_near(s.radius(circ_b).unwrap(), 2.0, "mirrored radius");
    assert_pt(&s, bc, 5.0, -5.0);
    assert_pt(&s, bs, 5.0, -3.0);
    assert_pt(&s, be, 3.0, -5.0);
}

#[test]
fn distance_dimensions() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    s.add_constraint(FixPoint(a)).unwrap();
    let b = s.add_point(3.0, 4.0);
    let d = s.add_constraint(Distance { a, b, value: 10.0 }).unwrap();
    let c = s.add_point(-2.0, 1.0);
    let h = s
        .add_constraint(HorizontalDistance {
            a,
            b: c,
            value: 7.0,
        })
        .unwrap();
    let e = s.add_point(1.0, -2.0);
    let v = s
        .add_constraint(VerticalDistance {
            a,
            b: e,
            value: 3.0,
        })
        .unwrap();
    solve_ok(&mut s);
    assert_pt(&s, b, 6.0, 8.0);
    assert_near(pt(&s, c)[0], -7.0, "horizontal distance keeps the side");
    assert_near(pt(&s, e)[1], -3.0, "vertical distance keeps the side");
    for id in [d, h, v] {
        assert_near(
            s.measure(id).unwrap(),
            s.dimension_value(id).unwrap(),
            "measure",
        );
    }
}

#[test]
fn point_line_and_line_line_distance() {
    let mut s = System::new();
    let (l, m) = two_lines(&mut s, [0.0, 0.0, 10.0, 0.0], [0.0, -2.0, 10.0, -3.0]);
    s.add_constraint(FixEntity(l)).unwrap();
    s.add_constraint(Parallel(l, m)).unwrap();
    s.add_constraint(LineDistance {
        a: l,
        b: m,
        value: 6.0,
    })
    .unwrap();
    let p = s.add_point(5.0, 1.0);
    s.add_constraint(PointLineDistance {
        point: p,
        line: l,
        value: 4.0,
    })
    .unwrap();
    solve_ok(&mut s);
    let (a, b) = ends(&s, m);
    assert_near(a[1], -6.0, "line below at 6");
    assert_near(b[1], -6.0, "parallel");
    assert_near(pt(&s, p)[1], 4.0, "point above at 4");
}

#[test]
fn length_and_angle() {
    let mut s = System::new();
    let o = s.add_point(0.0, 0.0);
    let x = s.add_point(10.0, 0.0);
    let base = s.add_line(o, x).unwrap();
    s.add_constraint(FixEntity(base)).unwrap();
    let t = s.add_point(5.0, 6.0);
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
            value: PI / 3.0,
        })
        .unwrap();
    solve_ok(&mut s);
    assert_pt(&s, t, 5.0, 10.0 * (PI / 3.0).sin());
    // A larger angle rotates the same way (no mirror flip).
    s.set_dimension_value(ang, 2.5).unwrap();
    solve_ok(&mut s);
    assert_pt(&s, t, 10.0 * 2.5f64.cos(), 10.0 * 2.5f64.sin());
}

#[test]
fn angle_quadrant_from_geometry() {
    // Lines at 120 degrees to each other; a 60 degree dimension measures the
    // supplementary quadrant and keeps it.
    let mut s = System::new();
    let o = s.add_point(0.0, 0.0);
    let x = s.add_point(10.0, 0.0);
    let base = s.add_line(o, x).unwrap();
    s.add_constraint(FixEntity(base)).unwrap();
    let t = s.add_point(-5.0, 8.66);
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
            value: PI / 3.0,
        })
        .unwrap();
    solve_ok(&mut s);
    assert_pt(&s, t, -5.0, 10.0 * (PI / 3.0).sin());
    s.set_dimension_value(ang, PI / 4.0).unwrap();
    solve_ok(&mut s);
    let h = 10.0 * FRAC_PI_4.cos();
    assert_pt(&s, t, -h, h);
}

#[test]
fn radius_diameter_arc_length() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let circle = s.add_circle(c, 3.0).unwrap();
    s.add_constraint(Radius {
        entity: circle,
        value: 4.0,
    })
    .unwrap();
    let c2 = s.add_point(20.0, 0.0);
    let circle2 = s.add_circle(c2, 3.0).unwrap();
    s.add_constraint(Diameter {
        entity: circle2,
        value: 10.0,
    })
    .unwrap();
    let ac = s.add_point(40.0, 0.0);
    let st = s.add_point(45.0, 0.0);
    let en = s.add_point(40.0, 5.0);
    let arc = s.add_arc(ac, st, en).unwrap();
    s.add_constraint(FixPoint(ac)).unwrap();
    s.add_constraint(FixPoint(st)).unwrap();
    let len = s
        .add_constraint(ArcLength {
            arc,
            value: 5.0 * PI,
        })
        .unwrap();
    solve_ok(&mut s);
    assert_near(s.radius(circle).unwrap(), 4.0, "radius");
    assert_near(s.radius(circle2).unwrap(), 5.0, "diameter");
    assert_pt(&s, en, 35.0, 0.0);
    assert_near(s.measure(len).unwrap(), 5.0 * PI, "arc length");
}

#[test]
fn ellipse_radii_and_elliptical_arc() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let a = s.add_point(8.0, 1.0);
    let e = s.add_ellipse(c, a, 3.0).unwrap();
    assert_eq!(s.entity_kind(e), Some(EntityKind::Ellipse));
    s.add_constraint(FixPoint(c)).unwrap();
    let axis = s.add_line(c, a).unwrap();
    s.add_constraint(Horizontal(axis)).unwrap();
    s.add_constraint(MajorRadius {
        ellipse: e,
        value: 10.0,
    })
    .unwrap();
    s.add_constraint(MinorRadius {
        ellipse: e,
        value: 4.0,
    })
    .unwrap();
    // An elliptical arc whose end points stay on its ellipse.
    let c2 = s.add_point(30.0, 0.0);
    let a2 = s.add_point(36.0, 0.0);
    let p0 = s.add_point(36.5, 0.5);
    let p1 = s.add_point(29.0, 3.5);
    let ea = s.add_elliptical_arc(c2, a2, 3.0, p0, p1).unwrap();
    s.add_constraint(FixEntity(ea)).unwrap();
    // A fixed end point that is off the fixed ellipse cannot hold.
    let fix = s.add_constraint(FixPoint(p0)).unwrap();
    let r = s.solve(&opts());
    assert!(!r.is_ok());
    assert!(r.conflicting().contains(&fix), "{r:?}");
    // The other part of the sketch was solved anyway.
    assert_pt(&s, a, 10.0, 0.0);
    assert_near(s.minor_radius(e).unwrap(), 4.0, "minor");
    s.remove_constraint(fix).unwrap();
    solve_ok(&mut s);
    // The fixed elliptical arc kept its ellipse; its end points moved onto it.
    assert_pt(&s, a2, 36.0, 0.0);
    assert_near(s.minor_radius(ea).unwrap(), 3.0, "fixed minor radius");
    for p in [p0, p1] {
        let v = pt(&s, p);
        let (x, y) = ((v[0] - 30.0) / 6.0, v[1] / 3.0);
        assert_near(x * x + y * y, 1.0, "end on ellipse");
    }
}

#[test]
fn elliptical_arc_end_points_on_ellipse() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let a = s.add_point(6.0, 0.0);
    let p0 = s.add_point(6.5, 0.5);
    let p1 = s.add_point(-1.0, 3.5);
    let ea = s.add_elliptical_arc(c, a, 3.0, p0, p1).unwrap();
    s.add_constraint(FixPoint(c)).unwrap();
    s.add_constraint(FixPoint(a)).unwrap();
    solve_ok(&mut s);
    let b = s.minor_radius(ea).unwrap();
    for p in [p0, p1] {
        let v = pt(&s, p);
        let (x, y) = (v[0] / 6.0, v[1] / b);
        assert_near(x * x + y * y, 1.0, "end on ellipse");
    }
}

#[test]
fn tangent_line_circle_keeps_side() {
    let mut s = System::new();
    let (l, _) = two_lines(&mut s, [0.0, 0.0, 10.0, 0.0], [0.0, 30.0, 1.0, 30.0]);
    s.add_constraint(FixEntity(l)).unwrap();
    let c = s.add_point(5.0, -4.0);
    let circle = s.add_circle(c, 2.0).unwrap();
    s.add_constraint(Tangent(l, circle)).unwrap();
    s.add_constraint(Radius {
        entity: circle,
        value: 3.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_near(pt(&s, c)[1], -3.0, "centre below the line at r");
}

#[test]
fn tangent_line_arc_at_shared_end_is_smooth() {
    let mut s = System::new();
    let p0 = s.add_point(0.0, 0.0);
    let p1 = s.add_point(10.0, 0.0);
    let l = s.add_line(p0, p1).unwrap();
    s.add_constraint(FixEntity(l)).unwrap();
    // Arc starting at the line's end, centre roughly above it.
    let c = s.add_point(10.5, 5.0);
    let e = s.add_point(15.0, 5.5);
    let arc = s.add_arc(c, p1, e).unwrap();
    s.add_constraint(Tangent(l, arc)).unwrap();
    s.add_constraint(Radius {
        entity: arc,
        value: 5.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_pt(&s, c, 10.0, 5.0);
    assert_near(dist(pt(&s, e), [10.0, 5.0]), 5.0, "end on arc");
    let an = s.analyze();
    assert!(an.redundant.is_empty() && an.conflicts.is_empty(), "{an:?}");
}

#[test]
fn tangent_circles_external_and_internal() {
    let mut s = System::new();
    let c1 = s.add_point(0.0, 0.0);
    let a = s.add_circle(c1, 5.0).unwrap();
    s.add_constraint(FixEntity(a)).unwrap();
    let c2 = s.add_point(9.0, 0.0);
    let b = s.add_circle(c2, 3.0).unwrap();
    s.add_constraint(Tangent(a, b)).unwrap();
    let c3 = s.add_point(1.0, 0.5);
    let c = s.add_circle(c3, 2.0).unwrap();
    s.add_constraint(Tangent(a, c)).unwrap();
    solve_ok(&mut s);
    assert_near(
        dist(pt(&s, c1), pt(&s, c2)),
        5.0 + s.radius(b).unwrap(),
        "external",
    );
    assert_near(
        dist(pt(&s, c1), pt(&s, c3)),
        5.0 - s.radius(c).unwrap(),
        "internal",
    );
}

#[test]
fn tangent_arcs_at_shared_end() {
    let mut s = System::new();
    let c1 = s.add_point(0.0, 0.0);
    let s1 = s.add_point(5.0, 0.0);
    let j = s.add_point(0.0, 5.0);
    let a1 = s.add_arc(c1, s1, j).unwrap();
    s.add_constraint(FixEntity(a1)).unwrap();
    // Second arc continues from j, centre roughly further along the radius.
    let c2 = s.add_point(0.5, 8.0);
    let e2 = s.add_point(-3.0, 8.0);
    let a2 = s.add_arc(c2, j, e2).unwrap();
    s.add_constraint(Tangent(a1, a2)).unwrap();
    solve_ok(&mut s);
    let v = pt(&s, c2);
    assert_near(v[0], 0.0, "centres collinear with the joint");
    let an = s.analyze();
    assert!(an.redundant.is_empty() && an.conflicts.is_empty(), "{an:?}");
}

#[test]
fn tangent_line_spline_end_and_interior() {
    let mut s = System::new();
    let ctrl: Vec<_> = [[0.0, 0.0], [4.0, 1.0], [8.0, 4.0], [12.0, 0.0]]
        .iter()
        .map(|c| s.add_point(c[0], c[1]))
        .collect();
    let sp = s.add_bspline(3, &ctrl, None, None).unwrap();
    s.add_constraint(FixPoint(ctrl[0])).unwrap();
    s.add_constraint(FixPoint(ctrl[3])).unwrap();
    let q = s.add_point(-10.0, 0.5);
    let l = s.add_line(q, ctrl[0]).unwrap();
    s.add_constraint(Horizontal(l)).unwrap();
    s.add_constraint(Tangent(l, sp)).unwrap();
    solve_ok(&mut s);
    let g = s.spline(sp).unwrap();
    assert_near(g.control_points[1][1], 0.0, "first control leg horizontal");
    // Interior tangency with a line above the curve.
    let (top, _) = two_lines(&mut s, [0.0, 5.0, 12.0, 5.0], [0.0, 50.0, 1.0, 50.0]);
    s.add_constraint(Horizontal(top)).unwrap();
    s.add_constraint(Tangent(top, sp)).unwrap();
    solve_ok(&mut s);
    let y = ends(&s, top).0[1];
    let max_y = (0..=4000)
        .map(|i| s.spline_point(sp, i as f64 / 4000.0).unwrap()[1])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        (max_y - y).abs() < 1e-4,
        "line touches the top of the curve: {max_y} vs {y}"
    );
}

#[test]
fn tangent_arc_spline_end() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let st = s.add_point(5.0, 0.0);
    let en = s.add_point(0.0, 5.0);
    let arc = s.add_arc(c, st, en).unwrap();
    s.add_constraint(FixEntity(arc)).unwrap();
    let ctrl = [
        en,
        s.add_point(-3.0, 6.0),
        s.add_point(-6.0, 4.0),
        s.add_point(-8.0, 8.0),
    ];
    let sp = s.add_bspline(3, &ctrl, None, None).unwrap();
    s.add_constraint(Tangent(arc, sp)).unwrap();
    solve_ok(&mut s);
    let g = s.spline(sp).unwrap();
    // The first leg is perpendicular to the radius at (0, 5): horizontal.
    assert_near(g.control_points[1][1], 5.0, "leg horizontal");
}

/// Signed curvature of a spline at `t` from finite differences of points.
fn spline_curvature(s: &System, sp: mitcad_solver::EntityId, t: f64) -> f64 {
    let h = 1e-4;
    let (t0, t1, t2) = if t + 2.0 * h <= 1.0 {
        (t, t + h, t + 2.0 * h)
    } else {
        (t - 2.0 * h, t - h, t)
    };
    let (a, b, c) = (
        s.spline_point(sp, t0).unwrap(),
        s.spline_point(sp, t1).unwrap(),
        s.spline_point(sp, t2).unwrap(),
    );
    // Curvature of the circle through three points (signed).
    let cr = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    2.0 * cr / (dist(a, b) * dist(b, c) * dist(a, c))
}

#[test]
fn smooth_spline_continues_an_arc_with_equal_curvature() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let st = s.add_point(5.0, 0.0);
    let en = s.add_point(0.0, 5.0);
    let arc = s.add_arc(c, st, en).unwrap();
    s.add_constraint(FixEntity(arc)).unwrap();
    let ctrl = [
        en,
        s.add_point(-3.0, 5.5),
        s.add_point(-6.0, 4.0),
        s.add_point(-9.0, 6.0),
        s.add_point(-12.0, 2.0),
    ];
    let sp = s.add_bspline(3, &ctrl, None, None).unwrap();
    s.add_constraint(FixPoint(ctrl[4])).unwrap();
    s.add_constraint(Smooth(arc, sp)).unwrap();
    solve_ok(&mut s);
    let g = s.spline(sp).unwrap();
    assert_near(g.control_points[1][1], 5.0, "G1: leg horizontal");
    // Traversing the arc counter-clockwise and on into the spline, the
    // curvature stays +1/5 (turning left).
    let k = spline_curvature(&s, sp, 0.0);
    assert!((k - 0.2).abs() < 1e-3, "curvature at the join {k}");
    let an = s.analyze();
    assert!(an.redundant.is_empty() && an.conflicts.is_empty(), "{an:?}");
}

#[test]
fn smooth_spline_after_a_line_starts_straight() {
    let mut s = System::new();
    let a = s.add_point(-10.0, 0.0);
    let b = s.add_point(0.0, 0.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(FixEntity(l)).unwrap();
    let ctrl = [
        b,
        s.add_point(4.0, 1.0),
        s.add_point(8.0, 3.0),
        s.add_point(12.0, 8.0),
    ];
    let sp = s.add_bspline(3, &ctrl, None, None).unwrap();
    s.add_constraint(FixPoint(ctrl[3])).unwrap();
    s.add_constraint(Smooth(sp, l)).unwrap();
    solve_ok(&mut s);
    let g = s.spline(sp).unwrap();
    // Zero curvature at a clamped cubic start: the first three control
    // points are collinear.
    assert_near(g.control_points[1][1], 0.0, "G1");
    assert_near(g.control_points[2][1], 0.0, "G2");
    assert!(s.add_constraint(Smooth(l, l)).is_err());
}

#[test]
fn fitted_spline_passes_through_fit_points() {
    let mut s = System::new();
    let f: Vec<_> = [[0.0, 0.0], [10.0, 5.0], [20.0, 0.0], [30.0, 5.0]]
        .iter()
        .map(|c| s.add_point(c[0], c[1]))
        .collect();
    let sp = s.add_fitted_spline(&f, [SplineEnd::Natural; 2]).unwrap();
    s.add_constraint(FixPoint(f[0])).unwrap();
    s.add_constraint(Distance {
        a: f[0],
        b: f[3],
        value: 40.0,
    })
    .unwrap();
    solve_ok(&mut s);
    // The curve passes through the moved fit point.
    let end = pt(&s, f[3]);
    assert_near(dist(end, [0.0, 0.0]), 40.0, "distance");
    let tail = s.spline_point(sp, 1.0).unwrap();
    assert_near(dist(tail, end), 0.0, "curve ends at the fit point");
    let g = s.spline(sp).unwrap();
    let mid = pt(&s, f[1]);
    let best = (0..=4000)
        .map(|i| {
            let t = i as f64 / 4000.0;
            dist(s.spline_point(sp, t).unwrap(), mid)
        })
        .fold(f64::INFINITY, f64::min);
    assert!(
        best < 1e-2,
        "through interior fit point ({best}); {} control points",
        g.control_points.len()
    );
}

#[test]
fn invalid_definitions_are_rejected() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(1.0, 0.0);
    assert!(s.add_line(a, a).is_err());
    assert!(s.add_circle(a, -1.0).is_err());
    let l = s.add_line(a, b).unwrap();
    assert!(matches!(
        s.add_constraint(Radius {
            entity: l,
            value: 1.0
        }),
        Err(Error::WrongEntityKind { .. })
    ));
    assert!(matches!(
        s.add_constraint(Distance { a, b, value: -1.0 }),
        Err(Error::InvalidValue(_))
    ));
    let id = s.add_constraint(Horizontal(l)).unwrap();
    assert_eq!(
        s.set_dimension_value(id, 3.0),
        Err(Error::NotADimension(id))
    );
    assert_eq!(s.remove_point(a), Err(Error::InUse));
    assert_eq!(s.remove_entity(l), Err(Error::InUse));
    s.remove_constraint(id).unwrap();
    s.remove_entity(l).unwrap();
    s.remove_point(a).unwrap();
    assert!(s.point(a).is_none());
}
