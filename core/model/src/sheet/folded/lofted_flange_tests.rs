// SPDX-License-Identifier: MIT
//! A flange on a Lofted Flange's facet (mitcad#2575): the strip of the
//! facet along its edge the bends are built on, within the facet.

use super::*;
use crate::ids::FeatureUid;

fn area(points: &[[f64; 2]]) -> f64 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(p, q)| p[0] * q[1] - q[0] * p[1])
        .sum::<f64>()
        / 2.0
}

/// A triangle facet on its edge from (0, 0) to (80, 0), its apex 76 up.
const TRIANGLE: [[f64; 2]; 3] = [[0.0, 0.0], [80.0, 0.0], [40.0, 76.0]];

#[test]
fn a_band_is_clipped_to_the_facet() {
    let band = vec![[0.0, 0.0], [80.0, 0.0], [80.0, 2.0], [0.0, 2.0]];
    let strip = clipped(band, &TRIANGLE);
    assert_eq!(strip.len(), 4, "{strip:?}");
    // A trapezoid: the edge, 2 up, its sides along the triangle's.
    let top = 80.0 * (1.0 - 2.0 / 76.0);
    assert!((area(&strip) - (80.0 + top)).abs() < 1e-9, "{strip:?}");
    assert!(strip.iter().all(|p| in_outline(&TRIANGLE, *p, 1e-9)));
    assert!(!in_outline(&TRIANGLE, [-1.0, 1.0], 1e-9));
}

#[test]
fn the_strip_keeps_the_edge_and_lies_in_the_facet() {
    let edge = MaterialEdge {
        piece: super::super::super::generated(FeatureUid(5), 1_400_001),
        curve: BoundedCurve2::from_geometry(SegmentGeometry::Line {
            start: [80.0, 0.0],
            end: [0.0, 0.0],
        })
        .expect("a line"),
    };
    let region = strip(
        FeatureUid(9),
        &edge,
        ([80.0, 0.0], [0.0, 0.0]),
        &TRIANGLE,
        2.0,
    )
    .expect("a strip");
    let edges = &region.loops[0].edges;
    assert_eq!(edges.len(), 4);
    assert_eq!(edges.iter().filter(|e| e.piece == edge.piece).count(), 1);
    let top = 80.0 * (1.0 - 2.0 / 76.0);
    let expected = (80.0 + top) / 2.0 * 2.0;
    let found = super::super::super::develop::region_area(&region).expect("an area");
    assert!((found - expected).abs() < 1e-9, "{found} {expected}");
}
