// SPDX-License-Identifier: MIT
//! Equal spacing (mitcad#221): successive intervals between anchors equal,
//! with an optional common spacing, in each measurement mode.

mod common;

use common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::{Anchor, EntityId, Error, PointId, SpacingMode, System};

fn points(s: &mut System, at: &[[f64; 2]]) -> Vec<PointId> {
    at.iter().map(|p| s.add_point(p[0], p[1])).collect()
}

fn anchors(points: &[PointId]) -> Vec<Anchor> {
    points.iter().map(|p| Anchor::Point(*p)).collect()
}

/// A fixed line from `a` to `b` (a direction).
fn fixed_line(s: &mut System, a: [f64; 2], b: [f64; 2]) -> EntityId {
    let p = s.add_point(a[0], a[1]);
    let q = s.add_point(b[0], b[1]);
    let l = s.add_line(p, q).unwrap();
    s.add_constraint(FixEntity(l)).unwrap();
    l
}

#[test]
fn horizontal_spacing_equalises_x_and_leaves_y_and_the_size_free() {
    let mut s = System::new();
    let p = points(&mut s, &[[0.0, 0.0], [9.0, 5.0], [21.0, -3.0], [30.0, 7.0]]);
    s.add_constraint(FixPoint(p[0])).unwrap();
    s.add_constraint(EqualSpacing {
        anchors: anchors(&p),
        mode: SpacingMode::Horizontal,
    })
    .unwrap();
    solve_ok(&mut s);
    let x: Vec<f64> = p.iter().map(|q| pt(&s, *q)[0]).collect();
    assert_near(x[1] - x[0], x[2] - x[1], "first and second interval");
    assert_near(x[2] - x[1], x[3] - x[2], "second and third interval");
    // No alignment: the y coordinates stay where they were.
    for (q, y) in p.iter().zip([0.0, 5.0, -3.0, 7.0]) {
        assert_near(pt(&s, *q)[1], y, "y untouched");
    }
    // 8 unknowns - 2 (fixed) - 2 (two interval equations): the common
    // spacing and the y coordinates stay free.
    let a = s.analyze();
    assert_eq!(a.dof, 4);
    assert!(a.redundant.is_empty() && a.conflicts.is_empty());
}

#[test]
fn a_common_spacing_drives_every_interval_and_changes_them_all() {
    let mut s = System::new();
    let p = points(&mut s, &[[0.0, 0.0], [9.0, 0.0], [21.0, 0.0]]);
    s.add_constraint(FixPoint(p[0])).unwrap();
    s.add_constraint(EqualSpacing {
        anchors: anchors(&p),
        mode: SpacingMode::Horizontal,
    })
    .unwrap();
    let spacing = s
        .add_constraint(Spacing {
            a: Anchor::Point(p[0]),
            b: Anchor::Point(p[1]),
            mode: SpacingMode::Horizontal,
            value: 10.0,
        })
        .unwrap();
    solve_ok(&mut s);
    assert_near(pt(&s, p[1])[0], 10.0, "first interval");
    assert_near(pt(&s, p[2])[0], 20.0, "second interval");
    s.set_dimension_value(spacing, 25.0).unwrap();
    solve_ok(&mut s);
    assert_near(pt(&s, p[1])[0], 25.0, "first interval after the edit");
    assert_near(pt(&s, p[2])[0], 50.0, "second interval after the edit");
    assert_near(s.measure(spacing).unwrap(), 25.0, "measured spacing");
    assert_eq!(
        s.set_dimension_value(spacing, 0.0),
        Err(Error::InvalidValue("the dimension must be positive"))
    );
}

#[test]
fn vertical_spacing_keeps_the_order_downwards() {
    let mut s = System::new();
    // Listed top to bottom: the intervals are negative and stay so.
    let p = points(&mut s, &[[1.0, 30.0], [4.0, 22.0], [-2.0, 5.0]]);
    s.add_constraint(FixPoint(p[0])).unwrap();
    s.add_constraint(EqualSpacing {
        anchors: anchors(&p),
        mode: SpacingMode::Vertical,
    })
    .unwrap();
    s.add_constraint(Spacing {
        a: Anchor::Point(p[0]),
        b: Anchor::Point(p[1]),
        mode: SpacingMode::Vertical,
        value: 12.0,
    })
    .unwrap();
    solve_ok(&mut s);
    assert_pt(&s, p[1], 4.0, 18.0);
    assert_pt(&s, p[2], -2.0, 6.0);
}

#[test]
fn spacing_along_a_line_projects_on_its_direction() {
    let mut s = System::new();
    let dir = fixed_line(&mut s, [0.0, -10.0], [3.0, -6.0]); // direction (0.6, 0.8)
    let p = points(&mut s, &[[0.0, 0.0], [6.0, 2.0], [7.0, 14.0]]);
    s.add_constraint(FixPoint(p[0])).unwrap();
    s.add_constraint(EqualSpacing {
        anchors: anchors(&p),
        mode: SpacingMode::Along(dir),
    })
    .unwrap();
    s.add_constraint(Spacing {
        a: Anchor::Point(p[0]),
        b: Anchor::Point(p[1]),
        mode: SpacingMode::Along(dir),
        value: 10.0,
    })
    .unwrap();
    solve_ok(&mut s);
    let along = |q: PointId| {
        let v = pt(&s, q);
        0.6 * v[0] + 0.8 * v[1]
    };
    assert_near(along(p[1]), 10.0, "first along the line");
    assert_near(along(p[2]), 20.0, "second along the line");
    // Across the line the points keep their offsets.
    let across = |q: PointId| {
        let v = pt(&s, q);
        -0.8 * v[0] + 0.6 * v[1]
    };
    assert_near(across(p[1]), -0.8 * 6.0 + 0.6 * 2.0, "first across");
    assert_near(across(p[2]), -0.8 * 7.0 + 0.6 * 14.0, "second across");
}

#[test]
fn the_direction_line_is_a_reference_the_spacing_never_turns() {
    // A free direction line (one end fixed) and far anchors: the spacing
    // moves the anchors along the line, it does not turn the line.
    let mut s = System::new();
    let d0 = s.add_point(0.0, -10.0);
    let d1 = s.add_point(3.0, -6.0);
    let dir = s.add_line(d0, d1).unwrap();
    s.add_constraint(FixPoint(d0)).unwrap();
    let p = points(&mut s, &[[0.0, 0.0], [30.0, 40.0], [62.0, 80.0]]);
    s.add_constraint(FixPoint(p[0])).unwrap();
    s.add_constraint(EqualSpacing {
        anchors: anchors(&p),
        mode: SpacingMode::Along(dir),
    })
    .unwrap();
    let spacing = s
        .add_constraint(Spacing {
            a: Anchor::Point(p[0]),
            b: Anchor::Point(p[1]),
            mode: SpacingMode::Along(dir),
            value: 60.0,
        })
        .unwrap();
    solve_ok(&mut s);
    assert_pt(&s, d1, 3.0, -6.0);
    let along = |s: &System, q: PointId| {
        let v = pt(s, q);
        0.6 * v[0] + 0.8 * v[1]
    };
    assert_near(along(&s, p[1]), 60.0, "first along the line");
    assert_near(along(&s, p[2]), 120.0, "second along the line");
    // Turned by another constraint, the line carries the spacing with it.
    s.set_dimension_value(spacing, 50.0).unwrap();
    s.add_constraint(Vertical(dir)).unwrap();
    solve_ok(&mut s);
    let (a, b) = ends(&s, dir);
    assert_near(a[0], b[0], "the line turned vertical");
    let up = if b[1] > a[1] { 1.0 } else { -1.0 };
    assert_near(
        up * (pt(&s, p[1])[1] - pt(&s, p[0])[1]),
        50.0,
        "first along the turned line",
    );
    assert_near(
        up * (pt(&s, p[2])[1] - pt(&s, p[1])[1]),
        50.0,
        "second along the turned line",
    );
}

#[test]
fn fixed_directions_never_drag_the_anchors_across() {
    // An arc kept at its radius makes the solve iterate; the anchors still
    // move only along the fixed direction (mitcad#221).
    let mut s = System::new();
    let dir = fixed_line(&mut s, [0.0, -30.0], [3.0, -26.0]);
    let p1 = s.add_point(0.0, 0.0);
    let l0 = s.add_point(8.0, -4.0);
    let l1 = s.add_point(12.0, 4.0);
    s.add_line(l0, l1).unwrap();
    let c = s.add_point(26.0, 3.0);
    s.add_circle(c, 2.0).unwrap();
    let ac = s.add_point(33.0, -2.0);
    let as_ = s.add_point(38.0, -2.0);
    let ae = s.add_point(33.0, 3.0);
    let arc = s.add_arc(ac, as_, ae).unwrap();
    s.add_constraint(FixPoint(p1)).unwrap();
    s.add_constraint(Radius {
        entity: arc,
        value: 5.0,
    })
    .unwrap();
    s.add_constraint(EqualSpacing {
        anchors: vec![
            Anchor::Point(p1),
            Anchor::Mid(l0, l1),
            Anchor::Point(c),
            Anchor::Point(ac),
        ],
        mode: SpacingMode::Along(dir),
    })
    .unwrap();
    s.add_constraint(Spacing {
        a: Anchor::Point(p1),
        b: Anchor::Mid(l0, l1),
        mode: SpacingMode::Along(dir),
        value: 10.0,
    })
    .unwrap();
    solve_ok(&mut s);
    let across = |v: [f64; 2]| -0.8 * v[0] + 0.6 * v[1];
    assert_near(across(pt(&s, l0)), across([8.0, -4.0]), "line start across");
    assert_near(across(pt(&s, l1)), across([12.0, 4.0]), "line end across");
    assert_near(across(pt(&s, c)), across([26.0, 3.0]), "center across");
    assert_near(s.radius(arc).unwrap(), 5.0, "the arc keeps its radius");
}

#[test]
fn point_to_point_spacing_makes_a_chain_of_equal_distances() {
    let mut s = System::new();
    let p = points(
        &mut s,
        &[[0.0, 0.0], [10.0, 0.0], [14.0, 7.0], [10.0, 20.0]],
    );
    s.add_constraint(FixPoint(p[0])).unwrap();
    s.add_constraint(EqualSpacing {
        anchors: anchors(&p),
        mode: SpacingMode::PointToPoint,
    })
    .unwrap();
    s.add_constraint(Spacing {
        a: Anchor::Point(p[0]),
        b: Anchor::Point(p[1]),
        mode: SpacingMode::PointToPoint,
        value: 12.0,
    })
    .unwrap();
    solve_ok(&mut s);
    for w in p.windows(2) {
        assert_near(dist(pt(&s, w[0]), pt(&s, w[1])), 12.0, "link of the chain");
    }
    // Not collinear: a chain, not a row.
    let (a, b, c) = (pt(&s, p[0]), pt(&s, p[1]), pt(&s, p[2]));
    let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    assert!(cross.abs() > 1.0, "the chain was straightened: {cross}");
}

#[test]
fn mixed_anchors_use_centres_and_line_midpoints_only() {
    let mut s = System::new();
    // A point, the midpoint of a line, the centre of a circle.
    let a = s.add_point(0.0, 0.0);
    let l0 = s.add_point(8.0, -4.0);
    let l1 = s.add_point(12.0, 4.0);
    let line = s.add_line(l0, l1).unwrap();
    let c = s.add_point(26.0, 3.0);
    let circle = s.add_circle(c, 2.0).unwrap();
    s.add_constraint(FixPoint(a)).unwrap();
    s.add_constraint(Length { line, value: 9.0 }).unwrap();
    s.add_constraint(EqualSpacing {
        anchors: vec![Anchor::Point(a), Anchor::Mid(l0, l1), Anchor::Point(c)],
        mode: SpacingMode::Horizontal,
    })
    .unwrap();
    s.add_constraint(Spacing {
        a: Anchor::Point(a),
        b: Anchor::Mid(l0, l1),
        mode: SpacingMode::Horizontal,
        value: 15.0,
    })
    .unwrap();
    solve_ok(&mut s);
    let (p0, p1) = ends(&s, line);
    assert_near((p0[0] + p1[0]) / 2.0, 15.0, "the line's midpoint");
    assert_near(pt(&s, c)[0], 30.0, "the circle's center");
    // Sizes are their own: the line keeps its length, the circle its radius.
    assert_near(line_len(&s, line), 9.0, "line length");
    assert_near(s.radius(circle).unwrap(), 2.0, "radius");
}

#[test]
fn a_second_equal_spacing_on_the_same_anchors_is_redundant() {
    let mut s = System::new();
    let p = points(&mut s, &[[0.0, 0.0], [10.0, 1.0], [20.0, 2.0]]);
    s.add_constraint(EqualSpacing {
        anchors: anchors(&p),
        mode: SpacingMode::Horizontal,
    })
    .unwrap();
    let again = s
        .add_constraint(EqualSpacing {
            anchors: anchors(&p),
            mode: SpacingMode::Horizontal,
        })
        .unwrap();
    solve_ok(&mut s);
    assert_eq!(s.analyze().redundant_constraints(), vec![again]);
}

#[test]
fn a_contradicting_distance_conflicts_with_the_spacing() {
    let mut s = System::new();
    let p = points(&mut s, &[[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]]);
    s.add_constraint(FixPoint(p[0])).unwrap();
    let equal = s
        .add_constraint(EqualSpacing {
            anchors: anchors(&p),
            mode: SpacingMode::Horizontal,
        })
        .unwrap();
    let spacing = s
        .add_constraint(Spacing {
            a: Anchor::Point(p[0]),
            b: Anchor::Point(p[1]),
            mode: SpacingMode::Horizontal,
            value: 10.0,
        })
        .unwrap();
    let total = s
        .add_constraint(HorizontalDistance {
            a: p[0],
            b: p[2],
            value: 30.0,
            signed: false,
        })
        .unwrap();
    let r = s.solve(&opts());
    assert!(!r.is_ok());
    let conflicting = r.conflicting();
    for id in [equal, spacing, total] {
        assert!(conflicting.contains(&id), "{id:?} not in {conflicting:?}");
    }
}

#[test]
fn bad_equal_spacings_are_refused() {
    let mut s = System::new();
    let p = points(&mut s, &[[0.0, 0.0], [10.0, 0.0]]);
    assert!(
        s.add_constraint(EqualSpacing {
            anchors: anchors(&p),
            mode: SpacingMode::Horizontal,
        })
        .is_err(),
        "two anchors make no equal spacing"
    );
    let c = s.add_point(5.0, 5.0);
    let circle = s.add_circle(c, 1.0).unwrap();
    let q = s.add_point(20.0, 0.0);
    assert!(matches!(
        s.add_constraint(EqualSpacing {
            anchors: vec![Anchor::Point(p[0]), Anchor::Point(p[1]), Anchor::Point(q)],
            mode: SpacingMode::Along(circle),
        }),
        Err(Error::WrongEntityKind { .. })
    ));
}
