// SPDX-License-Identifier: MIT
//! Offsets of chains whose short pieces curve tighter than the distance
//! (mitcad#1938): what folds over is left out and the rest meets where it
//! crosses, so that every point of the result lies the distance from the
//! chain, the result is connected, and only a chain of which nothing is
//! left is refused.

use std::f64::consts::{FRAC_PI_2, PI};

use crate::geometry::{Curve2, Nurbs, P2, dist};
use crate::offset::{Computed, Join, Made, compute, made_curve};

fn line(a: P2, b: P2) -> (Curve2, bool) {
    (Curve2::Line { a, b }, true)
}

/// An arc about `center` from angle `from` to `to` (radians), the chain
/// running that way (counter-clockwise when `to > from`).
fn arc(center: P2, radius: f64, from: f64, to: f64) -> (Curve2, bool) {
    let (start, end) = if to > from { (from, to) } else { (to, from) };
    (
        Curve2::Arc {
            center,
            radius,
            start,
            end,
        },
        to > from,
    )
}

fn polar(center: P2, radius: f64, angle: f64) -> P2 {
    [
        center[0] + radius * angle.cos(),
        center[1] + radius * angle.sin(),
    ]
}

/// A bend about `center` from angle `from` to `to` as `segments` short
/// lines: a polyline of a curve.
fn polyline_bend(
    center: P2,
    radius: f64,
    from: f64,
    to: f64,
    segments: usize,
) -> Vec<(Curve2, bool)> {
    (0..segments)
        .map(|k| {
            let at = |k: usize| from + (to - from) * k as f64 / segments as f64;
            line(
                polar(center, radius, at(k)),
                polar(center, radius, at(k + 1)),
            )
        })
        .collect()
}

/// A 30 x 20 outline counter-clockwise with its corners rounded by
/// `radius[k]`, as arcs or as polylines of `segments` lines.
fn rounded_outline(radius: [f64; 4], segments: Option<usize>) -> Vec<(Curve2, bool)> {
    let (w, h) = (30.0, 20.0);
    // Corner centres and the angle each corner starts at.
    let corners = [
        ([w - radius[0], radius[0]], -FRAC_PI_2),
        ([w - radius[1], h - radius[1]], 0.0),
        ([radius[2], h - radius[2]], FRAC_PI_2),
        ([radius[3], radius[3]], PI),
    ];
    let mut chain = Vec::new();
    for (k, (center, from)) in corners.iter().enumerate() {
        let before = corners[(k + 3) % 4];
        let start = polar(before.0, radius[(k + 3) % 4], before.1 + FRAC_PI_2);
        let end = polar(*center, radius[k], *from);
        chain.push(line(start, end));
        let to = from + FRAC_PI_2;
        match segments {
            Some(n) => chain.extend(polyline_bend(*center, radius[k], *from, to, n)),
            None => chain.push(arc(*center, radius[k], *from, to)),
        }
    }
    chain
}

/// An open S: a line, a left bend of 1.5 mm as a polyline, a line up, a
/// right bend as a tangent arc of 1 mm, and a line on.
fn s_chain() -> Vec<(Curve2, bool)> {
    let mut chain = vec![line([0.0, 0.0], [15.0, 0.0])];
    chain.extend(polyline_bend([15.0, 1.5], 1.5, -FRAC_PI_2, 0.0, 6));
    chain.push(line([16.5, 1.5], [16.5, 16.5]));
    chain.push(arc([17.5, 16.5], 1.0, PI, FRAC_PI_2));
    chain.push(line([17.5, 17.5], [32.5, 17.5]));
    chain
}

/// A cubic Bezier as a B-spline.
fn bezier(control: [P2; 4]) -> Curve2 {
    Curve2::Nurbs(Nurbs {
        degree: 3,
        control: control.to_vec(),
        weights: Vec::new(),
        knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
    })
}

/// A V of one spline whose bottom curves at a radius of about 1.9 mm.
fn v_spline() -> Curve2 {
    bezier([[-10.0, 10.0], [0.0, -10.0], [0.0, -10.0], [10.0, 10.0]])
}

fn name(i: usize) -> String {
    format!("c{}", i + 1)
}

/// The distance from `p` to the nearest curve of the chain.
fn to_chain(chain: &[(Curve2, bool)], p: P2) -> f64 {
    chain
        .iter()
        .map(|(c, _)| c.closest(p).1)
        .fold(f64::INFINITY, f64::min)
}

/// Every point of every curve the offset made lies `d` from the chain (no
/// fold-over: a folded loop comes nearer), each curve runs between the
/// ends it lists, and the curves follow each other: each end meets the
/// next curve's start, or a round corner arc between them.
fn assert_true_offset(chain: &[(Curve2, bool)], d: f64, c: &Computed) {
    let tol = 1e-3;
    let what = format!("offset {d}: {c:?}");
    assert!(!c.made.is_empty(), "{what}");
    let curves = c
        .made
        .iter()
        .chain(c.corners.iter().map(|(_, arc)| arc))
        .map(made_curve);
    for curve in curves {
        let (lo, hi) = curve.domain();
        for k in 0..=200 {
            let p = curve.point(lo + (hi - lo) * f64::from(k) / 200.0);
            let at = to_chain(chain, p);
            assert!(
                (at - d.abs()).abs() <= tol,
                "{p:?} lies {at} from the chain; {what}"
            );
        }
    }
    for (made, (from, to)) in c.made.iter().zip(&c.ends) {
        let curve = made_curve(made);
        let (lo, hi) = curve.domain();
        let ends = [curve.point(lo), curve.point(hi)];
        let closed = dist(*from, *to) <= 1e-9;
        if !matches!(made, Made::Circle { .. }) && !closed {
            let fits = (dist(ends[0], *from) <= tol && dist(ends[1], *to) <= tol)
                || (dist(ends[0], *to) <= tol && dist(ends[1], *from) <= tol);
            assert!(fits, "{ends:?} against {from:?} {to:?}; {what}");
        }
    }
    let links: Vec<(usize, usize)> = c
        .next
        .iter()
        .enumerate()
        .filter_map(|(j, after)| Some((j, (*after)?)))
        .collect();
    assert_eq!(links.len(), c.joins.len(), "{what}");
    for ((j, after), join) in links.iter().zip(&c.joins) {
        let (end, start) = (c.ends[*j].1, c.ends[*after].0);
        if *join == Join::Round {
            let arc = c
                .corners
                .iter()
                .find(|(k, _)| k == j)
                .map(|(_, arc)| made_curve(arc))
                .unwrap_or_else(|| panic!("no corner arc at {j}; {what}"));
            let (lo, hi) = arc.domain();
            let ends = [arc.point(lo), arc.point(hi)];
            assert!(
                ends.iter().any(|e| dist(*e, end) <= tol)
                    && ends.iter().any(|e| dist(*e, start) <= tol),
                "{what}"
            );
        } else {
            assert!(dist(end, start) <= 1e-9, "{end:?} {start:?}; {what}");
        }
    }
}

/// The offset by each distance on both sides, each a true offset.
fn offset_both_ways(chain: &[(Curve2, bool)], closed: bool, distances: &[f64]) {
    for &distance in distances {
        for d in [distance, -distance] {
            let made = compute(chain, d, closed, None, &name)
                .unwrap_or_else(|e| panic!("offset {d}: {e}"));
            assert_true_offset(chain, d, &made);
            if closed {
                // One outline round the whole: the curves follow each other
                // back to the first.
                let mut at = 0;
                for _ in 0..made.made.len() {
                    at = made.next[at].unwrap_or_else(|| panic!("open at {at}: {made:?}"));
                }
                assert_eq!(at, 0, "{made:?}");
            }
        }
    }
}

#[test]
fn an_outline_with_polyline_corners_offsets_past_their_radius() {
    // Corners of 1.5 mm as six short lines each.
    let chain = rounded_outline([1.5; 4], Some(6));
    offset_both_ways(&chain, true, &[2.7, 5.0]);
    // Inward the corners vanish and the sides meet: four lines.
    let inner = compute(&chain, -5.0, true, None, &name).unwrap();
    assert_eq!(inner.made.len(), 4, "{inner:?}");
}

#[test]
fn an_outline_with_tight_corner_arcs_offsets_past_their_radius() {
    let chain = rounded_outline([1.0, 2.0, 1.0, 2.0], None);
    offset_both_ways(&chain, true, &[2.7, 5.0]);
    let inner = compute(&chain, -2.7, true, None, &name).unwrap();
    assert_eq!(inner.dropped, vec![1, 3, 5, 7], "{inner:?}");
}

#[test]
fn an_open_chain_of_short_bends_offsets_on_both_sides() {
    let chain = s_chain();
    offset_both_ways(&chain, false, &[2.7, 5.0]);
}

#[test]
fn a_short_tight_arc_between_lines_vanishes_into_their_corner() {
    // An L with a 1 mm arc in its corner, offset into the corner.
    let chain = vec![
        line([0.0, 0.0], [20.0, 0.0]),
        arc([20.0, 1.0], 1.0, -FRAC_PI_2, 0.0),
        line([21.0, 1.0], [21.0, 20.0]),
    ];
    for d in [-2.7, -5.0] {
        let made = compute(&chain, d, false, None, &name).unwrap();
        assert_eq!(made.dropped, vec![1], "{made:?}");
        assert_true_offset(&chain, d, &made);
        // The lines meet the distance in from both.
        let corner = made.ends[0].1;
        assert!(dist(corner, [21.0 + d, -d]) <= 1e-6, "{corner:?}");
    }
}

#[test]
fn a_spline_with_a_tight_bend_is_trimmed_where_its_sides_cross() {
    let chain = vec![(v_spline(), true)];
    for d in [-2.7, -5.0] {
        // Into the V: the bottom folds over, its sides meet on the axis.
        let made =
            compute(&chain, d, false, None, &name).unwrap_or_else(|e| panic!("offset {d}: {e}"));
        assert_true_offset(&chain, d, &made);
        assert_eq!(made.made.len(), 1, "{made:?}");
        let Made::Spline { knots, .. } = &made.made[0] else {
            panic!("{made:?}");
        };
        // One corner: a knot twice.
        let corners = knots.windows(2).filter(|w| w[0] == w[1]).count();
        assert_eq!(corners, 1, "{knots:?}");
        // The corner lies on the V's axis.
        let curve = made_curve(&made.made[0]);
        let (lo, hi) = curve.domain();
        let on_axis = (0..=400).any(|k| {
            let p = curve.point(lo + (hi - lo) * f64::from(k) / 400.0);
            p[0].abs() <= 1e-3
        });
        assert!(on_axis, "{made:?}");
    }
    // Away from the bend nothing folds.
    for d in [2.7, 5.0] {
        let made = compute(&chain, d, false, None, &name).unwrap();
        assert_true_offset(&chain, d, &made);
    }
}

#[test]
fn a_chain_with_a_tight_spline_offsets_on_both_sides() {
    // A line into the V's left side and one out of its right side.
    let chain = vec![
        line([-20.0, 30.0], [-10.0, 10.0]),
        (v_spline(), true),
        line([10.0, 10.0], [20.0, 30.0]),
    ];
    offset_both_ways(&chain, false, &[2.7, 5.0]);
}

#[test]
fn a_closed_ellipse_offset_past_its_ends_radius_is_a_lens() {
    // Its ends curve at a radius of 0.4 mm.
    let chain = vec![(
        Curve2::Ellipse {
            center: [0.0, 0.0],
            major: 10.0,
            minor: 2.0,
            rotation: 0.0,
        },
        true,
    )];
    let made = compute(&chain, -1.0, false, None, &name).unwrap_or_else(|e| panic!("{e}"));
    assert_true_offset(&chain, -1.0, &made);
    assert_eq!(made.made.len(), 1, "{made:?}");
    let Made::Spline { knots, control } = &made.made[0] else {
        panic!("{made:?}");
    };
    // Closed, with a corner at each end of the lens: one inside the knots
    // and one where it closes.
    assert!(dist(control[0], control[control.len() - 1]) <= 1e-9);
    let corners = knots.windows(2).filter(|w| w[0] == w[1]).count();
    assert_eq!(corners, 1, "{knots:?}");
    // Past its half width nothing is left: refused.
    assert!(compute(&chain, -3.0, false, None, &name).is_err());
}

#[test]
fn a_corner_knot_is_a_triple_knot_and_reads_back() {
    use crate::offset::spline_nurbs;
    let knots = [0.0, 0.25, 0.5, 0.5, 0.75, 1.0];
    // Two parts of two spans each: 2 * 3 + 2 control points each, less the
    // shared corner.
    let control: Vec<P2> = (0..11).map(|k| [f64::from(k), 0.0]).collect();
    let nurbs = spline_nurbs(&knots, &control);
    assert_eq!(nurbs.knots.len(), control.len() + 4);
    assert_eq!(nurbs.knots.iter().filter(|k| **k == 0.5).count(), 3);
    let kind = crate::EntityKind::Spline {
        degree: 3,
        control: Vec::new(),
        weights: Vec::new(),
        knots: nurbs.knots,
    };
    assert_eq!(crate::offset::stored_knots(&kind), Some(knots.to_vec()));
}
