// SPDX-License-Identifier: MIT
//! The geometry of a Lofted Flange (mitcad#2512): transitions matched and
//! triangulated, facets, seams and gaps, depths and developments against
//! their exact areas and lengths.

use super::*;

const Z: V3 = [0.0, 0.0, 1.0];

/// A rectangle `w` by `h` about (`cx`, `cy`) at height `z`, anticlockwise.
fn rectangle(cx: f64, cy: f64, z: f64, w: f64, h: f64) -> Chain {
    let p = |x: f64, y: f64| [cx + x, cy + y, z];
    let (a, b, c, d) = (
        p(-w / 2.0, -h / 2.0),
        p(w / 2.0, -h / 2.0),
        p(w / 2.0, h / 2.0),
        p(-w / 2.0, h / 2.0),
    );
    chain(
        vec![
            Piece::Line(a, b),
            Piece::Line(b, c),
            Piece::Line(c, d),
            Piece::Line(d, a),
        ],
        Z,
        "test",
    )
    .expect("a chain")
}

/// A circle of radius `r` about (`cx`, `cy`) at height `z`, starting at
/// angle `start`.
fn circle(cx: f64, cy: f64, z: f64, r: f64, start: f64) -> Chain {
    chain(
        vec![Piece::Arc(ArcPiece {
            center: [cx, cy, z],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            radius: r,
            from: start,
            to: start + TAU,
        })],
        Z,
        "test",
    )
    .expect("a chain")
}

fn shoelace(points: &[P2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (p, q) = (points[i], points[(i + 1) % n]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * b.abs().max(1.0)
}

#[test]
fn chains_join_curves_end_to_end_from_any_first_curve() {
    // An open chain picked from its middle runs on both ways.
    let a = [0.0, 0.0, 0.0];
    let b = [10.0, 0.0, 0.0];
    let c = [10.0, 10.0, 0.0];
    let d = [0.0, 10.0, 0.0];
    let open = chain(
        vec![Piece::Line(b, c), Piece::Line(a, b), Piece::Line(d, c)],
        Z,
        "first",
    )
    .expect("joined");
    assert!(!open.closed);
    assert_eq!(open.pieces.len(), 3);
    assert!(near(open.pieces[0].start(), a) || near(open.pieces[0].start(), d));
    assert!(rectangle(0.0, 0.0, 0.0, 10.0, 5.0).closed);
    assert!(circle(0.0, 0.0, 0.0, 5.0, 0.0).closed);
    // Curves that do not touch are refused.
    assert!(chain(vec![Piece::Line(a, b), Piece::Line(c, d)], Z, "first").is_err());
}

#[test]
fn a_rectangle_to_an_offset_rectangle_is_four_exact_trapezoids() {
    let a = rectangle(0.0, 0.0, 0.0, 100.0, 60.0);
    let b = aligned(&a, rectangle(10.0, 5.0, 80.0, 60.0, 40.0)).expect("aligned");
    let strips = matched(&a, &b).expect("matched");
    assert_eq!(strips.len(), 4);
    assert!(strips.iter().all(Strip::flat));
    // Each side: the parallel sides' mean times their distance apart.
    let trapezoid = |l1: f64, l2: f64, dy: f64, dz: f64| (l1 + l2) / 2.0 * dy.hypot(dz);
    // The sides at y = -30 and 30 against -15 and 25, at x = 50 and -50
    // against 40 and -20.
    let exact = trapezoid(100.0, 60.0, 15.0, 80.0)
        + trapezoid(100.0, 60.0, 5.0, 80.0)
        + trapezoid(60.0, 40.0, 10.0, 80.0)
        + trapezoid(60.0, 40.0, 30.0, 80.0);
    let t = transition(strips, true, false).expect("a transition");
    assert!(close(flat_area(&t, 0.0), exact, 1e-12));
    // Laid flat, the four trapezoids keep their area.
    let flat = developed(&t, 0.0).expect("developed");
    assert!(close(shoelace(&outline_points(&flat)), exact, 1e-12));
    // The corners fold away from the material (outwards): their insides on
    // the front, the angles negative.
    assert_eq!(t.joins.len(), 3);
    assert!(t.joins.iter().all(|j| j.inside_front && j.angle < 0.0));
    assert!(t.check_depth(2.0).is_ok());
}

#[test]
fn concentric_circles_develop_into_an_annular_sector() {
    // A frustum: radii 50 and 30, 40 high; slant s = sqrt(2000).
    let a = circle(0.0, 0.0, 0.0, 50.0, 0.0);
    let b = aligned(&a, circle(0.0, 0.0, 40.0, 30.0, 1.0)).expect("aligned");
    let strips = matched(&a, &b).expect("matched");
    assert_eq!(strips.len(), 1, "one cone between the two circles");
    let s = 2000.0_f64.sqrt();
    let (rho_a, rho_b) = (50.0 * s / 20.0, 30.0 * s / 20.0);
    let t = transition(strips, true, false).expect("a transition");
    let flat = developed(&t, 0.0).expect("developed");
    let Developed::Curved { rulings, .. } = &flat[0] else {
        panic!("a curved band");
    };
    // The apex: where the first and the last ruling meet when extended.
    let (a0, b0) = rulings[0];
    let (a1, b1) = rulings[SPANS / 2];
    let (d0, d1) = (sub2(b0, a0), sub2(b1, a1));
    let k = cross2(sub2(a1, a0), d1) / cross2(d0, d1);
    let apex = add2(a0, scale2(d0, k));
    for (pa, pb) in rulings {
        assert!(close(len2(sub2(*pa, apex)), rho_a, 1e-9), "{pa:?}");
        assert!(close(len2(sub2(*pb, apex)), rho_b, 1e-9), "{pb:?}");
    }
    // The sector's angle: the whole circle's length over its distance from
    // the apex.
    let (first, last) = (rulings[0].0, rulings[rulings.len() - 1].0);
    // (Less than half a turn here, either way round.)
    let turned = {
        let (u, v) = (sub2(first, apex), sub2(last, apex));
        cross2(u, v).atan2(u[0] * v[0] + u[1] * v[1]).abs()
    };
    assert!(close(turned, TAU * 20.0 / s, 1e-9), "{turned}");
}

#[test]
fn eccentric_circles_keep_their_lengths_laid_flat() {
    // An oblique cone: radii 50 and 30, the second 15 off the axis, 60 up.
    let a = circle(0.0, 0.0, 0.0, 50.0, 0.0);
    let b = aligned(&a, circle(15.0, 0.0, 60.0, 30.0, 0.0)).expect("aligned");
    let strips = matched(&a, &b).expect("matched");
    assert!(strips.iter().all(|s| !s.flat()));
    let t = transition(strips, true, false).expect("a transition");
    let flat = developed(&t, 0.0).expect("developed");
    // Every developed ruling as long as in the body.
    let rulings = neutral_rulings(&t, &flat, 0.0);
    for (model, chart, _, _) in &rulings {
        let d3 = distance(model[0], model[1]);
        let d2 = len2(sub2(chart[1], chart[0]));
        assert!(close(d2, d3, 1e-9), "{d2} against {d3}");
    }
    // The first profile's development as long as the circle: its spline
    // sampled finely.
    let curves = outline(&flat);
    let mut length = 0.0;
    for c in &curves {
        if let SegmentGeometry::BSpline { .. } = c {
            let bounded = crate::sheet::BoundedCurve2::from_geometry(c.clone()).expect("a curve");
            let (t0, t1) = (bounded.range.first, bounded.range.last);
            let n = 20000;
            let mut last = bounded.point(t0).expect("a point");
            for i in 1..=n {
                let p = bounded
                    .point(t0 + (t1 - t0) * i as f64 / n as f64)
                    .expect("a point");
                length += len2(sub2(p, last));
                last = p;
            }
        }
    }
    // Both circles' developments: 2 pi (50 + 30).
    assert!(close(length, TAU * 80.0, 1e-6), "{length}");
}

#[test]
fn a_rectangle_to_a_circle_is_triangles_and_corner_cones() {
    let a = rectangle(0.0, 0.0, 0.0, 250.0, 80.0);
    let b = aligned(&a, circle(0.0, 0.0, 100.0, 37.5, 0.0)).expect("aligned");
    let strips = matched(&a, &b).expect("matched");
    let flat = strips.iter().filter(|s| s.flat()).count();
    let cones = strips.iter().filter(|s| !s.flat()).count();
    assert_eq!((flat, cones), (4, 4), "{strips:?}");
    // Press brake: each corner's cone in 4 facets, every facet exact.
    let facets = faceted(&strips, Some(4), None);
    assert_eq!(facets.len(), 4 + 16);
    let t = transition(facets, true, false).expect("a transition");
    let laid = developed(&t, 0.0).expect("developed");
    assert!(close(
        shoelace(&outline_points(&laid)),
        flat_area(&t, 0.0),
        1e-12
    ));
    // More facets: a larger area, nearer the smooth surface's.
    let finer = transition(faceted(&strips, Some(16), None), true, false).expect("a transition");
    assert!(flat_area(&finer, 0.0) > flat_area(&t, 0.0));
    // A chord tolerance: as many facets as keep them within it.
    assert_eq!(facet_count(None, Some(0.1), 37.5, PI / 2.0), 11);
    // Material inwards cannot follow the rectangle's sharp corners.
    let inwards = transition(strips, true, true).expect("a transition");
    assert!(inwards.check_depth(2.0).is_err());
}

#[test]
fn a_seam_opens_both_profiles_by_the_gap() {
    let a = rectangle(0.0, 0.0, 0.0, 100.0, 60.0);
    let b = aligned(&a, rectangle(10.0, 5.0, 80.0, 60.0, 40.0)).expect("aligned");
    let strips = matched(&a, &b).expect("matched");
    let (k, s) = seam_place(&strips, None);
    let opened = opened_at(&strips, k, s);
    let gap = 2.0;
    let open = trimmed_end(
        &trimmed_start(&opened, gap / 2.0, gap / 2.0).expect("trimmed"),
        gap / 2.0,
        gap / 2.0,
    )
    .expect("trimmed");
    let along = |side: fn(&Strip) -> Side| open.iter().map(|s| side(s).length()).sum::<f64>();
    assert!(close(along(|s| s.a), 320.0 - gap, 1e-12));
    assert!(close(along(|s| s.b), 200.0 - gap, 1e-12));
    // The seam at a fraction of the first profile: through that point.
    let (k, s) = seam_place(&strips, Some(0.5 * 320.0));
    let point = strips[k].a.point(s);
    assert!(distance(point, [50.0, 30.0, 0.0]) < 1e-9, "{point:?}");
}

#[test]
fn a_rectangle_to_a_circle_opens_at_its_shortest_line() {
    let a = rectangle(0.0, 0.0, 0.0, 250.0, 80.0);
    let b = aligned(&a, circle(0.0, 0.0, 100.0, 37.5, 0.0)).expect("aligned");
    let strips = matched(&a, &b).expect("matched");
    let (k, s) = seam_place(&strips, None);
    // The middle of a long side, under the circle.
    let (p, q) = strips[k].ruling(s);
    assert!(
        p[0].abs() < 1e-6 && (p[1].abs() - 40.0).abs() < 1e-9,
        "{p:?}"
    );
    assert!((distance(p, q) - (2.5_f64.powi(2) + 100.0_f64.powi(2)).sqrt()).abs() < 1e-6);
    let open = trimmed_end(
        &trimmed_start(&opened_at(&strips, k, s), 0.5, 0.5).expect("trimmed"),
        0.5,
        0.5,
    )
    .expect("trimmed");
    let a_length: f64 = open.iter().map(|s| s.a.length()).sum();
    let b_length: f64 = open.iter().map(|s| s.b.length()).sum();
    assert!(close(a_length, 660.0 - 1.0, 1e-12));
    assert!(close(b_length, TAU * 37.5 - 1.0, 1e-12));
    // Die formed: still developable; the slight folds beside the seam
    // fold away from the material.
    let t = transition(open, true, false).expect("a transition");
    assert!(t.check_depth(2.0).is_ok());
    assert!(t.joins.iter().all(|j| j.angle <= 0.0));
    let laid = developed(&t, 1.0).expect("developed");
    assert!(!overlaps(&outline_points(&laid)));
}

#[test]
fn profiles_on_angled_planes_are_triangulated() {
    // A square on the ground, another tilted 30 degrees above it.
    let a = rectangle(0.0, 0.0, 0.0, 60.0, 60.0);
    let (s, c) = (30.0_f64.to_radians()).sin_cos();
    let tilt = |p: V3| [p[0], p[1] * c, 80.0 + p[1] * s];
    let pieces = rectangle(0.0, 0.0, 0.0, 40.0, 40.0)
        .pieces
        .iter()
        .map(|p| Piece::Line(tilt(p.start()), tilt(p.end())))
        .collect();
    let b = aligned(&a, chain(pieces, [0.0, -s, c], "second").expect("a chain")).expect("aligned");
    assert!(matched(&a, &b).is_err());
    let pa = polyline(&a, None, None);
    let pb = polyline(&b, None, None);
    let strips = triangulated(&polyline_from(&pa, 0.0), &polyline_from(&pb, 0.0));
    assert_eq!(strips.len(), 8);
    let t = transition(strips, true, false).expect("a transition");
    let laid = developed(&t, 0.0).expect("developed");
    assert!(close(
        shoelace(&outline_points(&laid)),
        flat_area(&t, 0.0),
        1e-12
    ));
}

#[test]
fn a_profile_that_turns_both_ways_is_not_matched() {
    // An L: its inner corner turns right.
    let p = |x: f64, y: f64| [x, y, 0.0];
    let l = chain(
        vec![
            Piece::Line(p(0.0, 0.0), p(20.0, 0.0)),
            Piece::Line(p(20.0, 0.0), p(20.0, 10.0)),
            Piece::Line(p(20.0, 10.0), p(10.0, 10.0)),
            Piece::Line(p(10.0, 10.0), p(10.0, 20.0)),
            Piece::Line(p(10.0, 20.0), p(0.0, 20.0)),
            Piece::Line(p(0.0, 20.0), p(0.0, 0.0)),
        ],
        Z,
        "first",
    )
    .expect("a chain");
    let b = aligned(&l, rectangle(10.0, 10.0, 50.0, 10.0, 10.0)).expect("aligned");
    let Err(Unmatched(why)) = matched(&l, &b) else {
        panic!("matched");
    };
    assert!(why.contains("convex"), "{why}");
}

#[test]
fn a_closed_profile_and_an_open_one_are_refused() {
    let a = rectangle(0.0, 0.0, 0.0, 10.0, 10.0);
    let open = chain(
        vec![Piece::Line([0.0, 0.0, 5.0], [10.0, 0.0, 5.0])],
        Z,
        "second",
    )
    .expect("a chain");
    assert!(aligned(&a, open).is_err());
}

#[test]
fn polylines_start_where_asked() {
    let square = [
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [10.0, 10.0, 0.0],
        [0.0, 10.0, 0.0],
    ];
    let from = polyline_from(&square, 15.0);
    assert_eq!(from.len(), 6);
    assert!(near(from[0], [10.0, 5.0, 0.0]));
    assert!(near(from[5], [10.0, 5.0, 0.0]));
    assert!(near(from[1], [10.0, 10.0, 0.0]));
    let corner = polyline_from(&square, 10.0);
    assert_eq!(corner.len(), 5);
    assert!(near(corner[0], [10.0, 0.0, 0.0]));
    assert!((polyline_length(&square, true) - 40.0).abs() < 1e-12);
    let (at, d) = nearest_on(&square, [12.0, 4.0, 0.0]);
    assert!((at - 14.0).abs() < 1e-12 && (d - 2.0).abs() < 1e-12);
}

#[test]
fn an_apex_on_the_second_profile_stays_one_point_laid_flat() {
    // A cone from a point 60 above over a third of a circle of radius 50
    // (the cones beside an eccentric transition's seam).
    let strip = Strip {
        a: Side::Arc(ArcPiece {
            center: [0.0, 0.0, 0.0],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            radius: 50.0,
            from: 0.0,
            to: TAU / 3.0,
        }),
        b: Side::Point([15.0, 0.0, 60.0]),
    };
    let t = transition(vec![strip], false, false).expect("a transition");
    let laid = developed(&t, 0.0).expect("developed");
    let Developed::Curved { rulings, .. } = &laid[0] else {
        panic!("a curved band");
    };
    let apex = rulings[0].1;
    assert!(rulings.iter().all(|r| len2(sub2(r.1, apex)) <= 1e-12));
    // Its developed edge along the arc as long as the arc.
    let length: f64 = rulings.windows(2).map(|w| len2(sub2(w[1].0, w[0].0))).sum();
    assert!(close(length, 50.0 * TAU / 3.0, 1e-4), "{length}");
    // Only the arc's development, the rulings and no edge at the apex.
    assert_eq!(outline(&laid).len(), 3);
}

/// An open chain of lines through the points (on a plane of normal Z).
fn lines(points: &[V3]) -> Chain {
    chain(
        points.windows(2).map(|w| Piece::Line(w[0], w[1])).collect(),
        Z,
        "test",
    )
    .expect("a chain")
}

/// The open L profiles of the CLI tests: (-50, 0)-(0, 0)-(0, 50) on XY to
/// (-30, 10)-(10, 10)-(10, 40) 60 up, two trapezoids folding along the
/// line from (0, 0, 0) to (10, 10, 60).
fn l_transition() -> Transition {
    let a = lines(&[[-50.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 50.0, 0.0]]);
    let b = aligned(
        &a,
        lines(&[[-30.0, 10.0, 60.0], [10.0, 10.0, 60.0], [10.0, 40.0, 60.0]]),
    )
    .expect("aligned");
    let strips = matched(&a, &b).expect("matched");
    assert_eq!(strips.len(), 2);
    transition(strips, false, false).expect("a transition")
}

/// The distance between two parallel chart lines.
fn apart(l1: [P2; 2], l2: [P2; 2]) -> f64 {
    let d = sub2(l1[1], l1[0]);
    cross2(d, sub2(l2[0], l1[0])).abs() / len2(d)
}

#[test]
fn press_brake_folds_bend_with_the_rules_radius() {
    // mitcad#2560: the L's fold has the material on its inside, so the
    // front is the bend's outside, radius r + t; each facet is cut back
    // (r + t) tan(θ / 2) along the fold, and the bend lies flat at the
    // middle (K 0.5) as wide as its arc there, (r + K t) θ.
    let t = l_transition();
    let fold = t.joins[0];
    assert!(fold.angle > 0.0 && !fold.inside_front);
    let theta = fold.angle;
    let (r, th, k) = (2.0, 2.0, 0.5);
    let planes = [([0.0; 3], Z), ([0.0, 0.0, 60.0], Z)];
    let (bent_t, bends) = bent(&t, r, th, k, planes, 2.0).expect("bent");
    assert_eq!(bends.len(), 1);
    assert_eq!(bends[0].strip, 1);
    assert_eq!(bent_t.strips.len(), 3);
    assert!(bent_t.strips[1].bend() && !bent_t.strips[1].flat());
    assert!(bent_t.joins.iter().all(|j| j.angle == 0.0));
    let ell = 3800f64.sqrt();
    let setback = (r + th) * (theta / 2.0).tan();
    let facets = 85.0 * 3700f64.sqrt() - 2.0 * setback * ell;
    assert!(close(flat_area(&bent_t, 0.0), facets, 1e-12));
    // The bend's ends lie in the profiles' planes.
    let bend = bent_t.strips[1];
    for s in [0.0, 0.3, 1.0] {
        assert!(bend.a.point(s)[2].abs() < 1e-9);
        assert!((bend.b.point(s)[2] - 60.0).abs() < 1e-9);
    }
    // Laid flat: the facets less their setbacks and the bend's arc.
    let exact = facets + (r + k * th) * theta * ell;
    let flat = developed(&bent_t, k * th).expect("developed");
    let area = shoelace(&outline_points(&flat));
    assert!(close(area, exact, 1e-8), "{area} against {exact}");
    let (centre, tangents) = bend_lines(&bent_t, &flat, 1).expect("the bend's lines");
    let wide = apart(tangents[0], tangents[1]);
    assert!(close(wide, (r + k * th) * theta, 1e-8), "{wide}");
    assert!(close(apart(tangents[0], centre), wide / 2.0, 1e-8));
    assert!(bent_t.check_depth(th).is_ok());
}

#[test]
fn bends_with_their_insides_on_the_front_keep_the_neutral_depth() {
    // A rectangle to an offset rectangle: the corners fold with their
    // insides on the front (the material outwards), front radius r.
    let a = rectangle(0.0, 0.0, 0.0, 100.0, 60.0);
    let b = aligned(&a, rectangle(10.0, 5.0, 80.0, 60.0, 40.0)).expect("aligned");
    let strips = matched(&a, &b).expect("matched");
    let trapezoid = |l1: f64, l2: f64, dy: f64, dz: f64| (l1 + l2) / 2.0 * dy.hypot(dz);
    let mut exact = trapezoid(100.0, 60.0, 15.0, 80.0)
        + trapezoid(100.0, 60.0, 5.0, 80.0)
        + trapezoid(60.0, 40.0, 10.0, 80.0)
        + trapezoid(60.0, 40.0, 30.0, 80.0);
    let t = transition(strips, true, false).expect("a transition");
    let (r, th, k) = (2.0, 1.0, 0.3);
    for (i, join) in t.joins.iter().enumerate() {
        assert!(join.inside_front);
        let theta = join.angle.abs();
        let (p, q) = t.strips[i].ruling(1.0);
        let ell = distance(p, q);
        exact += -2.0 * r * (theta / 2.0).tan() * ell + (r + k * th) * theta * ell;
    }
    let planes = [([0.0; 3], Z), ([0.0, 0.0, 80.0], Z)];
    let (bent_t, bends) = bent(&t, r, th, k, planes, 2.0).expect("bent");
    assert_eq!(bends.len(), 3);
    assert!(bent_t.shift.iter().all(|s| *s == 0.0));
    let flat = developed(&bent_t, k * th).expect("developed");
    let area = shoelace(&outline_points(&flat));
    assert!(close(area, exact, 1e-8), "{area} against {exact}");
    // With the material inwards the insides are on the back: the front
    // is the outside (radius r + t) and the bends lie flat at K t from
    // their insides, (1 - 2 K) t deeper than the facets' neutral surface.
    let strips = matched(&a, &b).expect("matched");
    let inwards = transition(strips, true, true).expect("a transition");
    let (bent_in, _) = bent(&inwards, r, th, k, planes, 2.0).expect("bent");
    for (s, shift) in bent_in.strips.iter().zip(&bent_in.shift) {
        let want = if s.bend() { (1.0 - 2.0 * k) * th } else { 0.0 };
        assert!((shift - want).abs() < 1e-12);
    }
}

/// The press brake rectangle 120 x 80 to a circle of radius 30 70 up of
/// the CLI tests, 4 facets a corner, opened at its shortest line with a
/// gap of 2 as the feature opens it with bends.
fn rectangle_to_circle_with_a_seam() -> Transition {
    let a = rectangle(0.0, 0.0, 0.0, 120.0, 80.0);
    let b = aligned(&a, circle(0.0, 0.0, 70.0, 30.0, 0.0)).expect("aligned");
    let smooth = matched(&a, &b).expect("matched");
    let (strip, s) = seam_place(&smooth, None);
    let opened = opened_at(&smooth, strip, s);
    let open = trimmed_end_with(
        &trimmed_start_with(&opened, 1.0, 1.0, false).expect("trimmed"),
        1.0,
        1.0,
        false,
    )
    .expect("trimmed");
    transition(faceted(&open, Some(4), None), true, false).expect("a transition")
}

#[test]
fn bends_meeting_in_a_point_open_a_corner_relief() {
    // mitcad#2576: the facets fan from the rectangle's corners and the
    // side triangles' tips lie on the circle; bends of radius 2 would run
    // into each other there. Each such point gets a corner relief: the
    // facets meeting it cut straight across, the bends ending on the cuts.
    let t = rectangle_to_circle_with_a_seam();
    let planes = [([0.0; 3], Z), ([0.0, 0.0, 70.0], Z)];
    let (r, th, k) = (2.0, 2.0, 0.5);
    // The rules' 2 mm is too small for the corners' fans: the least size
    // (6.37 mm at the corners beside the seam, worked out apart from the
    // model) is said in words.
    let err = bent(&t, r, th, k, planes, 2.0).expect_err("too small");
    assert!(
        err.message.contains("too small") && err.message.contains("at least 6.4 mm"),
        "{}",
        err.message
    );
    // Larger than the chords beside the circle's points: too large.
    let err = bent(&t, r, th, k, planes, 30.0).expect_err("too large");
    assert!(err.message.contains("too large"), "{}", err.message);
    let relief = 8.0;
    let (bent_t, bends) = bent(&t, r, th, k, planes, relief).expect("bent");
    // Every fold bent, the facets' pieces meeting flat or on bends.
    assert_eq!(
        bends.len(),
        t.joins.iter().filter(|j| j.angle != 0.0).count()
    );
    assert_eq!(bends.len(), 20);
    assert!(bent_t.joins.iter().all(|j| j.angle == 0.0));
    // No material within half the relief of the points where the folds met.
    let tips = [
        [60.0, -40.0, 0.0],
        [60.0, 40.0, 0.0],
        [-60.0, 40.0, 0.0],
        [-60.0, -40.0, 0.0],
        [30.0, 0.0, 70.0],
        [0.0, 30.0, 70.0],
        [-30.0, 0.0, 70.0],
    ];
    for s in &bent_t.strips {
        for p in s.points() {
            for tip in tips {
                assert!(distance(p, tip) > relief / 2.0, "{p:?} by {tip:?}");
            }
        }
    }
    assert!(bent_t.check_depth(th).is_ok());
    // Laid flat at the middle: the facets' areas (each moved whole) and
    // each bend's lines' lengths along its arc there, (r + K t) times the
    // angle; the expected blank worked out apart from the model.
    let flat = developed(&bent_t, k * th).expect("developed");
    let points = outline_points(&flat);
    assert!(!overlaps(&points));
    let area = shoelace(&points);
    let mut exact = flat_area(&bent_t, 0.0);
    for bend in &bends {
        let s = bent_t.strips[bend.strip];
        let Side::Section(c) = s.a else {
            panic!("a bend's side");
        };
        // Simpson's rule over its lines' lengths.
        let n = 64;
        let length = |i: usize| {
            let (p, q) = s.ruling(i as f64 / n as f64);
            distance(p, q)
        };
        let sum: f64 = (0..=n)
            .map(|i| {
                let w = if i == 0 || i == n {
                    1.0
                } else if i % 2 == 1 {
                    4.0
                } else {
                    2.0
                };
                w * length(i)
            })
            .sum();
        exact += (r + k * th) * (c.to - c.from).abs() * sum / (3.0 * n as f64);
    }
    assert!(close(area, exact, 1e-7), "{area} against {exact}");
    assert!(close(area, 21_616.688_249_737_94, 1e-7), "{area}");
}

#[test]
fn two_arcs_cut_at_one_angle_at_the_seam_leave_no_fan() {
    // Eccentric circles: with filling, the farther cut point's cone fills
    // the rest (a fan); without, both arcs are cut at the farther angle.
    let a = circle(0.0, 0.0, 0.0, 50.0, 0.0);
    let b = aligned(&a, circle(15.0, 0.0, 60.0, 30.0, 0.0)).expect("aligned");
    let smooth = matched(&a, &b).expect("matched");
    let (strip, s) = seam_place(&smooth, None);
    let opened = opened_at(&smooth, strip, s);
    let filled = trimmed_start(&opened, 1.0, 1.0).expect("trimmed");
    let cut = trimmed_start_with(&opened, 1.0, 1.0, false).expect("trimmed");
    assert!(filled.len() > cut.len());
    assert!(
        cut.iter()
            .all(|s| !s.a.is_point() && !s.b.is_point() && s.matched())
    );
}

#[test]
fn a_point_of_the_body_is_located_in_the_chart() {
    // The sharp L laid flat at K 0.3 of t 2: a circle about a point of its
    // second facet put on the front and the back and found again.
    let t = l_transition();
    let (th, k) = (2.0, 0.3);
    let neutral = k * th;
    let flat = developed(&t, neutral).expect("developed");
    let rulings = neutral_rulings(&t, &flat, neutral)
        .into_iter()
        .map(
            |(model, chart, front_to_back, depth)| crate::sheet::LoftedRuling {
                model,
                chart,
                front_to_back,
                depth: ((depth - neutral).abs() > 1e-12).then_some(depth),
            },
        )
        .collect();
    let l = crate::sheet::LoftedEmbedding {
        rulings,
        lines: Vec::new(),
        neutral_depth: neutral,
        k_factor: k,
        die_formed: false,
    };
    let Developed::Flat(c) = &flat[1] else {
        panic!("a facet");
    };
    let centre = scale2(add2(add2(c[0], c[1]), add2(c[2], c[3])), 0.25);
    let middle = super::super::chart_point(&l, centre, 0.0);
    for i in 0..16 {
        let a = TAU * i as f64 / 16.0;
        let p = add2(centre, [5.0 * a.cos(), 5.0 * a.sin()]);
        for depth in [0.0, th] {
            let x = super::super::chart_point(&l, p, depth);
            let at = super::super::located(&l, x, depth, 2.0 * th).expect("located");
            assert!(
                len2(sub2(at.chart, p)) < 1e-9,
                "{:?} against {p:?}",
                at.chart
            );
            assert!((at.depth - depth).abs() < 1e-9);
            // The facet's own map: exact, the same frame for every point.
            let d = sub(x, at.frame.origin);
            let by_frame = [dot(d, at.frame.x_axis), dot(d, at.frame.y_axis)];
            assert!(len2(sub2(by_frame, p)) < 1e-9);
        }
        // On the front the circle stays a circle.
        let front = super::super::chart_point(&l, p, 0.0);
        assert!((distance(front, middle) - 5.0).abs() < 1e-9);
    }
}

/// A square 60 x 60 on XY and a circle of radius 20 on a plane turned 30
/// degrees about X, aligned; the circle's centre and its plane's normal.
fn square_to_angled_circle() -> (Chain, Chain, V3, V3) {
    let a = rectangle(0.0, 86.602_540_378_443_86, 0.0, 60.0, 60.0);
    let (s, c) = (PI / 6.0).sin_cos();
    let (y, n) = ([0.0, c, s], [0.0, -s, c]);
    let b = chain(
        vec![Piece::Arc(ArcPiece {
            center: scale(y, 100.0),
            x: [1.0, 0.0, 0.0],
            y,
            radius: 20.0,
            from: 0.0,
            to: TAU,
        })],
        n,
        "test",
    )
    .expect("a chain");
    let b = aligned(&a, b).expect("aligned");
    (a, b, scale(y, 100.0), n)
}

/// The point at `t` of a B-spline curve of `degree` through `poles`, its
/// knots `knots` repeated `mults` times, by de Boor's algorithm.
fn curve_point(knots: &[f64], mults: &[u32], degree: u32, poles: &[V3], t: f64) -> V3 {
    let p = degree as usize;
    let knots: Vec<f64> = knots
        .iter()
        .zip(mults)
        .flat_map(|(k, m)| std::iter::repeat_n(*k, *m as usize))
        .collect();
    assert_eq!(knots.len(), poles.len() + p + 1, "knots and poles agree");
    let span = (p..poles.len())
        .rev()
        .find(|&k| knots[k] <= t)
        .expect("t in the knots' range");
    let mut d: Vec<V3> = poles[span - p..=span].to_vec();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = span - p + j;
            let alpha = (t - knots[i]) / (knots[i + p + 1 - r] - knots[i]);
            d[j] = add(scale(d[j - 1], 1.0 - alpha), scale(d[j], alpha));
        }
    }
    d[p]
}

/// The point of a patch at the fractions `fu` and `fv` of its knots'
/// ranges.
fn patch_at(patch: &SurfacePatch, fu: f64, fv: f64) -> V3 {
    assert_eq!(
        patch.poles.len(),
        patch.nu * patch.nv,
        "poles and counts agree"
    );
    let range = |k: &[f64], f: f64| k[0] + (k[k.len() - 1] - k[0]) * f;
    let (u, v) = (range(&patch.u_knots, fu), range(&patch.v_knots, fv));
    let across: Vec<V3> = (0..patch.nv)
        .map(|j| {
            let column: Vec<V3> = (0..patch.nu)
                .map(|i| patch.poles[i * patch.nv + j])
                .collect();
            curve_point(&patch.u_knots, &patch.u_mults, patch.u_degree, &column, u)
        })
        .collect();
    curve_point(&patch.v_knots, &patch.v_mults, patch.v_degree, &across, v)
}

/// The point of a patch's B-spline at `u` (its knots' parameter) and `v`
/// (0 to 1, straight across).
fn patch_point(patch: &SurfacePatch, u: f64, v: f64) -> V3 {
    let last = patch.u_knots[patch.u_knots.len() - 1];
    patch_at(patch, u / last, v)
}

#[test]
fn a_ruled_transition_made_solid_closes_where_its_backs_meet() {
    // mitcad#2560: the square to the circle on angled planes made solid 2
    // behind its front, both ways (one way its creases from the square's
    // corners turn away from the material and the backs are stretched to
    // where they meet, the other way they cross and are cut back to it),
    // every crease fading out to a smooth join at the circle. Its faces
    // close: each edge of a face, sampled, is an edge of another face, or
    // has no length. The backs are the bands' offsets, the patches within
    // a micrometre of them.
    let (a, b, ..) = square_to_angled_circle();
    let bands = ruled_transition(&a, &b, None, 2.0).expect("ruled");
    for depth in [2.0, -2.0] {
        let patches = ruled_body(&bands, depth, FeatureUid(22)).expect("a body");
        let samples = 9;
        let edges: Vec<(usize, Vec<V3>)> = patches
            .iter()
            .enumerate()
            .flat_map(|(n, patch)| {
                let side = |at: &dyn Fn(f64) -> V3| -> Vec<V3> {
                    (0..samples)
                        .map(|k| at(k as f64 / (samples - 1) as f64))
                        .collect()
                };
                [
                    side(&|f| patch_at(patch, 0.0, f)),
                    side(&|f| patch_at(patch, 1.0, f)),
                    side(&|f| patch_at(patch, f, 0.0)),
                    side(&|f| patch_at(patch, f, 1.0)),
                ]
                .into_iter()
                .map(move |points| (n, points))
            })
            .collect();
        let same = |p: &[V3], q: &[V3]| {
            p.iter().zip(q).all(|(x, y)| distance(*x, *y) <= 1e-5)
                || p.iter()
                    .zip(q.iter().rev())
                    .all(|(x, y)| distance(*x, *y) <= 1e-5)
        };
        for (n, points) in &edges {
            if points.iter().all(|p| distance(*p, points[0]) <= 1e-9) {
                continue;
            }
            assert!(
                edges.iter().any(|(m, other)| m != n && same(points, other)),
                "depth {depth}: an edge of {} is open: {:?}",
                patches[*n].face,
                points[0]
            );
        }
        let back = RuledBack::new(&bands, depth).expect("the backs meet");
        for (i, band) in bands.iter().enumerate() {
            let patch = patches
                .iter()
                .find(|p| {
                    p.face.role == "lofted_back" && p.face.key == Some(RoleKey::Index(i as u32))
                })
                .expect("a back");
            for s in [0.0, 0.25, 0.5, 0.75, 1.0] {
                for w in [0.0, 0.3, 0.5, 1.0] {
                    let (exact, [f, v]) = back.place(i, s, w).expect("on the back");
                    let off = distance(patch_at(patch, s, w), exact);
                    assert!(off < 1e-6, "depth {depth}: {off} mm off band {i}'s back");
                    let front = band.point(f, v);
                    assert!((distance(exact, front) - depth.abs()).abs() < 1e-9);
                }
            }
        }
        // Where two bands' backs meet, both are there.
        for i in 1..bands.len() {
            for w in [0.0, 0.2, 0.7, 0.99, 1.0] {
                let (end, start) = (back.point(i - 1, 1.0, w), back.point(i, 0.0, w));
                let gap = distance(end.expect("an end"), start.expect("a start"));
                assert!(gap < 1e-8, "depth {depth}: the backs {gap} mm apart at {w}");
            }
        }
    }
}

#[test]
fn a_ruled_transition_is_smooth_by_its_knots() {
    // mitcad#2560: the ruled front of a die formed transition laid flat by
    // triangulation is made solid by the kernel's offset, which refuses a
    // surface continuous only in its points by its knots (a cubic's inner
    // knots three times): its inner knots are twice, and it still runs
    // through the bands' points, at the stations and between them.
    let (a, b, ..) = square_to_angled_circle();
    let bands = ruled_transition(&a, &b, None, 2.0).expect("ruled");
    let patches = ruled_patches(&bands, FeatureUid(22));
    assert_eq!(patches.len(), bands.len());
    for (band, patch) in bands.iter().zip(&patches) {
        assert!(patch.u_mults.len() >= 2);
        let inner = &patch.u_mults[1..patch.u_mults.len() - 1];
        assert!(
            inner.iter().all(|&m| m < patch.u_degree),
            "inner knots {inner:?} of degree {}",
            patch.u_degree
        );
        let last = *patch.u_knots.last().expect("knots");
        for k in 0..=64 {
            let f = k as f64 / 64.0;
            for v in [0.0, 0.5, 1.0] {
                let off = distance(patch_point(patch, f * last, v), band.point(f, v));
                assert!(off < 1e-6, "{off} mm off the band at {f}, {v}");
            }
        }
    }
}

#[test]
fn a_ruled_transition_between_angled_profiles_lies_flat_by_triangles() {
    // mitcad#2560: a square 60 x 60 on XY to a circle of radius 20 on a
    // plane turned 30 degrees about X (no developable surface between
    // them): the ruled surface between the points at the same share of
    // both lengths, from the shortest line less 1 each side of it.
    let (a, b, centre, n) = square_to_angled_circle();
    assert!(matched(&a, &b).is_err());
    let gap = 2.0;
    let bands = ruled_transition(&a, &b, None, gap).expect("ruled");
    // A band between every two corners or seam ends: each smooth, each
    // joined to the next along a whole line.
    assert!(bands.len() >= 5);
    for w in bands.windows(2) {
        for v in [0.0, 0.5, 1.0] {
            assert!(distance(w[0].point(1.0, v), w[1].point(0.0, v)) < 1e-9);
        }
    }
    for band in &bands {
        for f in [0.0, 0.5, 1.0] {
            assert!(band.point(f, 0.0)[2].abs() < 1e-9);
            assert!(dot(sub(band.point(f, 1.0), centre), n).abs() < 1e-9);
            assert!((distance(band.point(f, 1.0), centre) - 20.0).abs() < 1e-9);
        }
    }
    // The profiles less the gap.
    let length = |side: fn(&RuledBand) -> &Run| -> f64 {
        bands
            .iter()
            .map(|band| {
                let run = side(band);
                (0..256)
                    .map(|i| {
                        distance(
                            run.point(i as f64 / 256.0),
                            run.point((i + 1) as f64 / 256.0),
                        )
                    })
                    .sum::<f64>()
            })
            .sum()
    };
    assert!(close(length(|b| &b.a), 240.0 - gap, 1e-9));
    assert!(close(length(|b| &b.b), 40.0 * PI - gap, 1e-4));
    // Laid flat by triangles (K 0: the front): the stated difference of
    // the lengths along its edges is the measured one, the pattern's first
    // rail against the first profile's, face by face.
    let shares = crate::sheet::folded::triangulated::strips(
        &bands
            .iter()
            .map(|b| distance(b.a.point(0.0), b.a.point(1.0)).max(1.0))
            .collect::<Vec<_>>(),
        32,
    );
    let fine = crate::sheet::folded::triangulated::FINE;
    let grids: Vec<_> = bands
        .iter()
        .zip(&shares)
        .map(|(band, &count)| ruled_grid(band, count * fine, 16, 1.0).expect("a grid"))
        .collect();
    let oriented: Vec<_> = grids
        .iter()
        .map(|grid| crate::sheet::folded::triangulated::Oriented {
            grid,
            orientation: crate::sheet::folded::triangulated::Orientation {
                transpose: false,
                reverse_along: false,
                reverse_across: false,
            },
        })
        .collect();
    let layout =
        crate::sheet::folded::triangulated::lay_flat(&oriented, 0.0, false, 0.0).expect("laid");
    assert!(layout.edges > 1e-3, "not developable: {}", layout.edges);
    let mut measured: f64 = 0.0;
    for (band, &(s0, s1)) in bands.iter().zip(&layout.spans) {
        for (rail, run) in [(0, &band.a), (1, &band.b)] {
            let pattern: f64 = (s0..s1)
                .map(|s| {
                    len2(sub2(
                        layout.flat[2 * (s + 1) + rail],
                        layout.flat[2 * s + rail],
                    ))
                })
                .sum();
            let faces: f64 = (0..4096)
                .map(|i| {
                    distance(
                        run.point(i as f64 / 4096.0),
                        run.point((i + 1) as f64 / 4096.0),
                    )
                })
                .sum();
            measured = measured.max((pattern - faces).abs());
        }
    }
    let last = layout.flat.len() / 2 - 1;
    let rung = |s: usize| len2(sub2(layout.flat[2 * s + 1], layout.flat[2 * s]));
    let first_band = &bands[0];
    let last_band = &bands[bands.len() - 1];
    measured = measured
        .max((rung(0) - distance(first_band.a.point(0.0), first_band.b.point(0.0))).abs())
        .max((rung(last) - distance(last_band.a.point(1.0), last_band.b.point(1.0))).abs());
    assert!(
        (layout.edges - measured).abs() < 1e-4,
        "stated {} against measured {measured}",
        layout.edges
    );
}
