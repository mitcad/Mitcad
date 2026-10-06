// SPDX-License-Identifier: MIT
//! Dragging geometry: goals are soft, constraints hard.

mod common;

use common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::{DragGoal, System};
use std::f64::consts::PI;

fn drag_to(s: &mut System, p: mitcad_solver::PointId, target: [f64; 2]) {
    let r = s.drag(&[DragGoal::Point { point: p, target }], &opts());
    assert!(r.is_ok(), "drag failed: {r:?}");
}

#[test]
fn free_point_follows_exactly() {
    let mut s = System::new();
    let p = s.add_point(0.0, 0.0);
    drag_to(&mut s, p, [3.0, -4.0]);
    assert_pt(&s, p, 3.0, -4.0);
}

#[test]
fn line_end_with_fixed_length_moves_on_a_circle() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 0.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(FixPoint(a)).unwrap();
    s.add_constraint(Length {
        line: l,
        value: 10.0,
    })
    .unwrap();
    // Pointer at (0, 20): the end goes to the nearest reachable point.
    drag_to(&mut s, b, [0.0, 20.0]);
    assert_pt(&s, b, 0.0, 10.0);
    assert_pt(&s, a, 0.0, 0.0);
    // Small steps as from a mouse.
    for i in 1..=20 {
        let ang = std::f64::consts::FRAC_PI_2 + i as f64 * 0.1;
        drag_to(&mut s, b, [12.0 * ang.cos(), 12.0 * ang.sin()]);
        assert_near(line_len(&s, l), 10.0, "length holds");
        let v = pt(&s, b);
        let diff = (v[1].atan2(v[0]) - ang + PI).rem_euclid(2.0 * PI) - PI;
        assert_near(diff, 0.0, "follows the pointer's direction");
    }
}

#[test]
fn point_on_fixed_line_slides_to_the_projection() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(10.0, 10.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(FixEntity(l)).unwrap();
    let p = s.add_point(1.0, 1.0);
    s.add_constraint(PointOnCurve(p, l)).unwrap();
    drag_to(&mut s, p, [8.0, 2.0]);
    assert_pt(&s, p, 5.0, 5.0);
}

#[test]
fn rectangle_corner_drag_resizes_and_keeps_the_rest() {
    let mut s = System::new();
    let r = rect(&mut s, 0.0, 0.0, 10.0, 5.0);
    s.add_constraint(FixPoint(r.p[0])).unwrap();
    drag_to(&mut s, r.p[2], [14.0, 8.0]);
    assert_pt(&s, r.p[0], 0.0, 0.0);
    assert_pt(&s, r.p[1], 14.0, 0.0);
    assert_pt(&s, r.p[2], 14.0, 8.0);
    assert_pt(&s, r.p[3], 0.0, 8.0);
}

#[test]
fn unconstrained_geometry_stays_put_while_dragging_other_parts() {
    let mut s = System::new();
    let r = rect(&mut s, 0.0, 0.0, 10.0, 5.0);
    let other = s.add_point(50.0, 50.0);
    // Dragging an edge's corner: the rectangle translates/resizes, the
    // opposite corner moves as little as possible.
    drag_to(&mut s, r.p[2], [12.0, 6.0]);
    assert_pt(&s, r.p[2], 12.0, 6.0);
    assert_pt(&s, other, 50.0, 50.0);
    let p0 = pt(&s, r.p[0]);
    assert!(
        dist(p0, [0.0, 0.0]) < 1e-6,
        "opposite corner did not move: {p0:?}"
    );
}

#[test]
fn fixed_point_cannot_be_dragged() {
    let mut s = System::new();
    let p = s.add_point(1.0, 2.0);
    s.add_constraint(FixPoint(p)).unwrap();
    let q = s.add_point(5.0, 2.0);
    s.add_constraint(Distance {
        a: p,
        b: q,
        value: 4.0,
    })
    .unwrap();
    drag_to(&mut s, p, [9.0, 9.0]);
    assert_pt(&s, p, 1.0, 2.0);
}

#[test]
fn dragging_a_circle_radius_and_a_tangent_line() {
    let mut s = System::new();
    let c = s.add_point(0.0, 0.0);
    let circle = s.add_circle(c, 5.0).unwrap();
    s.add_constraint(FixPoint(c)).unwrap();
    let a = s.add_point(-10.0, 5.0);
    let b = s.add_point(10.0, 5.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(Horizontal(l)).unwrap();
    s.add_constraint(Tangent(l, circle)).unwrap();
    let r = s.drag(
        &[DragGoal::Radius {
            entity: circle,
            target: 7.0,
        }],
        &opts(),
    );
    assert!(r.is_ok());
    assert_near(s.radius(circle).unwrap(), 7.0, "radius");
    assert_near(pt(&s, a)[1], 7.0, "tangent line follows");
}

#[test]
fn drag_with_spline_point_on_curve() {
    let mut s = System::new();
    let ctrl: Vec<_> = [[0.0, 0.0], [5.0, 5.0], [10.0, 5.0], [15.0, 0.0]]
        .iter()
        .map(|c| s.add_point(c[0], c[1]))
        .collect();
    let sp = s.add_bspline(3, &ctrl, None, None).unwrap();
    s.add_constraint(FixEntity(sp)).unwrap();
    let p = s.add_point(7.5, 3.75);
    s.add_constraint(PointOnCurve(p, sp)).unwrap();
    let target = [12.0, 10.0];
    drag_to(&mut s, p, target);
    let v = pt(&s, p);
    // On the curve, at the point of the curve nearest to the pointer.
    let samples: Vec<[f64; 2]> = (0..=4000)
        .map(|i| s.spline_point(sp, i as f64 / 4000.0).unwrap())
        .collect();
    let on = samples
        .iter()
        .map(|&q| dist(q, v))
        .fold(f64::INFINITY, f64::min);
    assert!(on < 1e-2);
    let nearest = samples
        .iter()
        .map(|&q| dist(q, target))
        .fold(f64::INFINITY, f64::min);
    assert!(
        (dist(v, target) - nearest).abs() < 1e-4,
        "{v:?}: {} vs {nearest}",
        dist(v, target)
    );
}
