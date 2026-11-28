// SPDX-License-Identifier: MIT
//! Notches in a Lofted Flange's chart (mitcad#2575): cut edges joined into
//! chains, their ends placed on the chart's boundary, and the boundary
//! between them that closes the material taken away.

use super::*;
use crate::ids::FeatureUid;

/// The square 10 x 10 at the origin, anticlockwise.
fn square() -> MaterialRegion {
    let mut region = super::super::super::polygon(
        FeatureUid(1),
        0,
        &[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
    )
    .expect("a square");
    super::super::super::develop::orient(&mut region).expect("oriented");
    region
}

fn line(a: [f64; 2], b: [f64; 2]) -> SegmentGeometry {
    SegmentGeometry::Line { start: a, end: b }
}

fn ends(curves: &[SegmentGeometry]) -> Vec<([f64; 2], [f64; 2])> {
    curves
        .iter()
        .map(|g| {
            BoundedCurve2::from_geometry(g.clone())
                .and_then(|c| c.ends())
                .expect("ends")
        })
        .collect()
}

#[test]
fn a_notch_is_an_open_chain_from_edge_to_edge() {
    // A U-shaped notch at the bottom edge, its pieces in any order and way.
    let curves = vec![
        line([6.0, 3.0], [4.0, 3.0]),
        line([4.0, 0.0], [4.0, 3.0]),
        line([6.0, 0.0], [6.0, 3.0]),
    ];
    let chains = joined(&ends(&curves));
    assert_eq!(chains.len(), 1);
    let chain = &chains[0];
    assert!(!chain.closed);
    assert_eq!(chain.items.len(), 3);
    let mut found = [chain.start, chain.end];
    found.sort_by(|a, b| a[0].total_cmp(&b[0]));
    assert!(
        near2(found[0], [4.0, 0.0]) && near2(found[1], [6.0, 0.0]),
        "{found:?}"
    );
    // A hole closes.
    let hole = vec![
        line([2.0, 2.0], [3.0, 2.0]),
        line([3.0, 3.0], [3.0, 2.0]),
        line([3.0, 3.0], [2.0, 2.0]),
    ];
    let chains = joined(&ends(&hole));
    assert_eq!(chains.len(), 1);
    assert!(chains[0].closed);
}

#[test]
fn a_notch_closes_along_the_boundary_between_its_ends() {
    let region = square();
    let regions = [region];
    let at = |p: [f64; 2]| {
        place_on(&regions, p)
            .expect("placed")
            .expect("on the boundary")
    };
    let (a, b) = (at([4.0, 0.0]), at([6.0, 0.0]));
    assert_eq!((a.region, a.lp, b.region, b.lp), (0, 0, 0, 0));
    // Anticlockwise, b lies ahead of a on the bottom edge.
    let lp = &regions[0].loops[0];
    let n = lp.edges.len() as f64;
    assert!(
        ((b.at - a.at).rem_euclid(n) - 0.2).abs() < 1e-12,
        "{a:?} {b:?}"
    );
    // From a on to b: the bottom edge between them.
    let pieces = along(lp, a.at, b.at).expect("along");
    assert_eq!(pieces.len(), 1);
    let (p, q) = BoundedCurve2::from_geometry(pieces[0].clone())
        .and_then(|c| c.ends())
        .expect("ends");
    let mut xs = [p[0], q[0]];
    xs.sort_by(f64::total_cmp);
    assert!((xs[0] - 4.0).abs() < 1e-12 && (xs[1] - 6.0).abs() < 1e-12);
    assert!(p[1].abs() < 1e-12 && q[1].abs() < 1e-12);
    // From b on round to a: the rest of the outline, past three corners.
    let rest = along(lp, b.at, a.at).expect("along");
    assert_eq!(rest.len(), 5);
    assert!(near2(loop_point(lp, a.at).expect("a point"), [4.0, 0.0]));
    // A point off the boundary has no place.
    assert!(place_on(&regions, [5.0, 5.0]).expect("placed").is_none());
}

#[test]
fn earlier_cuts_lie_on_the_boundary() {
    let regions = [square()];
    assert!(on_boundary(&line([2.0, 0.0], [7.0, 0.0]), &regions).expect("checked"));
    assert!(!on_boundary(&line([2.0, 0.0], [2.0, 3.0]), &regions).expect("checked"));
}
