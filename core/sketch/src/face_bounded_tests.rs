// SPDX-License-Identifier: MIT
//! Implicit supporting-face boundaries do not become sketch entities.

use std::f64::consts::{PI, TAU};

use crate::geometry::Curve2;
use crate::ids::{EntityUid, FeatureUid};
use crate::regions::{RegionCurve, face_bounded_regions};
use crate::topo::{CurveId, EdgeName, FaceName, RegionKey};

fn boundary(index: u32, radius: f64) -> RegionCurve {
    let edge = EdgeName::new(
        FaceName::new(FeatureUid(2), "end", None),
        FaceName::new(FeatureUid(2), "side", None),
    )
    .with_index(index);
    RegionCurve::new(
        CurveId::FaceEdge(Box::new(edge.into())),
        Curve2::Circle {
            center: [0.0, 0.0],
            radius,
            start: 0.0,
        },
    )
}

fn octagon(first: u32, apothem: f64) -> Vec<RegionCurve> {
    let radius = apothem / (PI / 8.0).cos();
    let point = |i: usize| {
        let angle = TAU * i as f64 / 8.0;
        [radius * angle.cos(), radius * angle.sin()]
    };
    (0..8)
        .map(|i| {
            RegionCurve::new(
                EntityUid(first + i as u32),
                Curve2::Line {
                    a: point(i),
                    b: point(i + 1),
                },
            )
        })
        .collect()
}

#[test]
fn a_circular_face_and_two_octagons_have_three_regions() {
    let mut curves = octagon(1, 20.0);
    curves.extend(octagon(9, 18.0));
    curves.push(boundary(0, 30.0));
    let (found, _) = face_bounded_regions(&curves);
    assert_eq!(found.len(), 3);
    assert_eq!(found.iter().filter(|r| r.profile.face_bounded()).count(), 1);
    let outside = found.iter().find(|r| r.profile.face_bounded()).unwrap();
    let octagon_area = 8.0 * 20.0_f64.powi(2) * (PI / 8.0).tan();
    assert!((outside.area - (PI * 900.0 - octagon_area)).abs() < 1e-6);
    assert_eq!(outside.profile.loops.len(), 2);
    let key = outside.profile.key.to_string();
    assert!(key.contains("b{E{"));
    assert_eq!(key.parse::<RegionKey>().unwrap(), outside.profile.key);
    let face = FaceName::end(FeatureUid(4), outside.profile.key.clone());
    assert_eq!(face.to_string().parse::<FaceName>().unwrap(), face);
}

#[test]
fn a_crossing_line_splits_the_face_and_keeps_a_sketch_region_outside() {
    let curves = vec![
        boundary(0, 30.0),
        RegionCurve::new(
            EntityUid(1),
            Curve2::Line {
                a: [-40.0, 0.0],
                b: [40.0, 0.0],
            },
        ),
        RegionCurve::new(
            EntityUid(2),
            Curve2::Circle {
                center: [50.0, 0.0],
                radius: 3.0,
                start: 0.0,
            },
        ),
    ];
    let (found, _) = face_bounded_regions(&curves);
    assert_eq!(found.len(), 3);
    for region in found.iter().filter(|r| r.profile.face_bounded()) {
        assert!((region.area - PI * 900.0 / 2.0).abs() < 1e-6);
        let text = region.profile.key.to_string();
        assert_eq!(text.parse::<RegionKey>().unwrap(), region.profile.key);
        for segment in region.profile.loops.iter().flat_map(|l| &l.segments) {
            let side = FaceName::side(FeatureUid(4), segment.key.clone());
            assert_eq!(side.to_string().parse::<FaceName>().unwrap(), side);
        }
    }
    assert_eq!(
        found.iter().filter(|r| !r.profile.face_bounded()).count(),
        1
    );
}

#[test]
fn a_face_hole_is_not_filled_by_an_implicit_profile() {
    let (found, _) = face_bounded_regions(&[boundary(0, 30.0), boundary(1, 5.0)]);
    assert_eq!(found.len(), 1);
    assert!((found[0].area - PI * (900.0 - 25.0)).abs() < 1e-6);
    assert_eq!(found[0].profile.loops.len(), 2);
}

#[test]
fn resizing_a_face_preserves_the_profile_key() {
    let (before, _) = face_bounded_regions(&[boundary(0, 30.0)]);
    let (after, _) = face_bounded_regions(&[boundary(0, 40.0)]);
    assert_eq!(before[0].profile.key, after[0].profile.key);
    assert!((after[0].area - PI * 1600.0).abs() < 1e-6);
}

fn rectangle(first: u32, lo: [f64; 2], hi: [f64; 2]) -> Vec<RegionCurve> {
    let corners = [lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]];
    (0..4)
        .map(|i| {
            RegionCurve::new(
                EntityUid(first + i as u32),
                Curve2::Line {
                    a: corners[i],
                    b: corners[(i + 1) % 4],
                },
            )
        })
        .collect()
}

#[test]
fn an_outside_closed_sketch_region_keeps_a_shared_face_edge() {
    // The stored rectangle touches the supporting face along the whole
    // right edge, or only its middle. The latter must keep the provenance
    // change at the two ends of the overlapping piece when joining edges.
    for (lo, hi, area) in [
        ([10.0, 0.0], [20.0, 10.0], 100.0),
        ([10.0, 2.0], [20.0, 8.0], 60.0),
    ] {
        let mut curves = rectangle(1, [0.0, 0.0], [10.0, 10.0]);
        for (index, curve) in curves.iter_mut().enumerate() {
            let edge = EdgeName::new(
                FaceName::new(FeatureUid(2), "end", None),
                FaceName::new(FeatureUid(2), "side", None),
            )
            .with_index(index as u32);
            curve.uid = CurveId::FaceEdge(Box::new(edge.into()));
        }
        curves.extend(rectangle(5, lo, hi));
        let (found, _) = face_bounded_regions(&curves);
        assert_eq!(found.len(), 2, "{found:?}");
        let own = found.iter().find(|r| r.centroid[0] > 10.0).unwrap();
        assert!((own.area - area).abs() < 1e-6);
        assert!(!own.profile.face_bounded());
        assert!(own.profile.key.segments().all(|s| !s.curve.is_face_edge()));
        let face = found.iter().find(|r| r.centroid[0] < 10.0).unwrap();
        assert!((face.area - 100.0).abs() < 1e-6);
        assert!(face.profile.face_bounded());
    }
}

#[test]
fn face_edges_beside_a_sketch_line_make_no_sliver() {
    // The face's top edge is two edges meeting 0.4 µm off the stored line
    // the sketch has along it (a projection of those edges deviates that
    // much, mitcad#1159): no region between the two, whose loop would make
    // no profile; the line stands for the face's edge.
    let face = |index: u32, a: [f64; 2], b: [f64; 2]| {
        let edge = EdgeName::new(
            FaceName::new(FeatureUid(2), "end", None),
            FaceName::new(FeatureUid(2), "side", None),
        )
        .with_index(index);
        RegionCurve::new(
            CurveId::FaceEdge(Box::new(edge.into())),
            Curve2::Line { a, b },
        )
    };
    let corner = [5.0, 10.0004];
    let mut curves = vec![
        face(0, [0.0, 0.0], [10.0, 0.0]),
        face(1, [10.0, 0.0], [10.0, 10.0]),
        face(2, [10.0, 10.0], corner),
        face(3, corner, [0.0, 10.0]),
        face(4, [0.0, 10.0], [0.0, 0.0]),
    ];
    curves.extend(rectangle(1, [0.0, 10.0], [10.0, 20.0]));
    let (found, _) = face_bounded_regions(&curves);
    assert_eq!(found.len(), 2, "{found:?}");
    for region in &found {
        assert!((region.area - 100.0).abs() < 1e-2, "{region:?}");
    }
    let face = found.iter().find(|r| r.centroid[1] < 10.0).unwrap();
    assert!(face.profile.face_bounded());
    let own = found.iter().find(|r| r.centroid[1] > 10.0).unwrap();
    assert!(own.profile.key.segments().all(|s| !s.curve.is_face_edge()));
}
