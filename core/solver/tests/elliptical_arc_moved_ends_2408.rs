// SPDX-License-Identifier: MIT
//! A fixed elliptical arc moved after it was added (mitcad#2408): a linked
//! projection's arc is built where the sketch had it and then moved to
//! where its source is now. Its end parameters, taken where it was added,
//! follow its moved ends with `seat_elliptical_arc_ends`; left where they
//! were, the arc's fix held them there and contradicted its fixed ends.

mod common;

use common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::{EntityId, PointId, System};

/// The point at parameter `t` of the ellipse about `c` with semi-axes `a`
/// along x and `b`.
fn on_ellipse(c: [f64; 2], a: f64, b: f64, t: f64) -> [f64; 2] {
    [c[0] + a * t.cos(), c[1] + b * t.sin()]
}

/// An elliptical arc about the origin, semi-axes 10 along x and 5, from
/// parameter 0.3 to 2.0, then its points and minor radius moved onto the
/// ellipse about (1, 0.5) with semi-axes 12 and 6, from 0.5 to 2.3:
/// the system, the arc and its centre, major point, start and end, with
/// the moved places.
fn moved_arc() -> (System, EntityId, [PointId; 4], [[f64; 2]; 4]) {
    let mut s = System::new();
    let start = on_ellipse([0.0, 0.0], 10.0, 5.0, 0.3);
    let end = on_ellipse([0.0, 0.0], 10.0, 5.0, 2.0);
    let points = [
        s.add_point(0.0, 0.0),
        s.add_point(10.0, 0.0),
        s.add_point(start[0], start[1]),
        s.add_point(end[0], end[1]),
    ];
    let arc = s
        .add_elliptical_arc(points[0], points[1], 5.0, points[2], points[3])
        .unwrap();
    let c = [1.0, 0.5];
    let moved = [
        c,
        [13.0, 0.5],
        on_ellipse(c, 12.0, 6.0, 0.5),
        on_ellipse(c, 12.0, 6.0, 2.3),
    ];
    for (p, at) in points.iter().zip(&moved) {
        s.set_point(*p, *at).unwrap();
    }
    s.set_minor_radius(arc, 6.0).unwrap();
    (s, arc, points, moved)
}

/// The arc's fix and its end points' fixes, as a sketch fixes a projected
/// arc.
fn fix(s: &mut System, arc: EntityId, points: &[PointId; 4]) {
    s.add_constraint(FixEntity(arc)).unwrap();
    s.add_constraint(FixPoint(points[2])).unwrap();
    s.add_constraint(FixPoint(points[3])).unwrap();
}

#[test]
fn a_fixed_elliptical_arc_moved_with_its_ends_seated_solves_where_it_is() {
    let (mut s, arc, points, moved) = moved_arc();
    s.seat_elliptical_arc_ends(arc).unwrap();
    fix(&mut s, arc, &points);
    solve_ok(&mut s);
    for (p, at) in points.iter().zip(&moved) {
        let now = pt(&s, *p);
        assert!(dist(now, *at) < 1e-9, "{now:?} stays at {at:?}");
    }
    assert!((s.minor_radius(arc).unwrap() - 6.0).abs() < 1e-12);
}

#[test]
fn a_fixed_elliptical_arc_moved_without_its_ends_seated_contradicts_itself() {
    // The cause of mitcad#2408: the end parameters of where it was added.
    let (mut s, arc, points, _) = moved_arc();
    fix(&mut s, arc, &points);
    let result = s.solve(&opts());
    assert!(
        !result.is_ok(),
        "the stale end parameters cannot hold: {result:?}"
    );
}

#[test]
fn seating_keeps_the_arc_sweep_across_the_parameter_seam() {
    // An arc from parameter 2.8 to 3.6 (across pi), moved a little: its
    // parameters turn by little, not by a whole turn.
    let mut s = System::new();
    let start = on_ellipse([0.0, 0.0], 10.0, 5.0, 2.8);
    let end = on_ellipse([0.0, 0.0], 10.0, 5.0, 3.6);
    let points = [
        s.add_point(0.0, 0.0),
        s.add_point(10.0, 0.0),
        s.add_point(start[0], start[1]),
        s.add_point(end[0], end[1]),
    ];
    let arc = s
        .add_elliptical_arc(points[0], points[1], 5.0, points[2], points[3])
        .unwrap();
    let moved_start = on_ellipse([0.0, 0.0], 10.0, 5.0, 2.85);
    let moved_end = on_ellipse([0.0, 0.0], 10.0, 5.0, 3.65);
    s.set_point(points[2], moved_start).unwrap();
    s.set_point(points[3], moved_end).unwrap();
    s.seat_elliptical_arc_ends(arc).unwrap();
    fix(&mut s, arc, &points);
    solve_ok(&mut s);
    assert!(dist(pt(&s, points[2]), moved_start) < 1e-9);
    assert!(dist(pt(&s, points[3]), moved_end) < 1e-9);
}

#[test]
fn seating_another_entity_changes_nothing() {
    let mut s = System::new();
    let a = s.add_point(0.0, 0.0);
    let b = s.add_point(3.0, 4.0);
    let line = s.add_line(a, b).unwrap();
    s.seat_elliptical_arc_ends(line).unwrap();
    assert!(dist(pt(&s, b), [3.0, 4.0]) < 1e-15);
}
