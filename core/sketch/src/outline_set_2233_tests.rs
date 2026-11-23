// SPDX-License-Identifier: MIT
//! The outline set of a face sketch (mitcad#2233): the face's edges and the
//! outline projected when the sketch was made never make the sketch's
//! curves cross ([`own_area_regions`]); the own curves still do.

use std::f64::consts::PI;

use crate::geometry::Curve2;
use crate::ids::{EntityUid, FeatureUid};
use crate::outlines::outline_set;
use crate::regions::{RegionCurve, own_area_regions};
use crate::topo::{CurveId, EdgeName, FaceName};

fn circle(id: u32, center: [f64; 2], radius: f64) -> RegionCurve {
    RegionCurve::new(
        EntityUid(id),
        Curve2::Circle {
            center,
            radius,
            start: 0.0,
        },
    )
}

/// The automatic boundaries of a circular face of radius 30 about the
/// origin: its edge, and with `outline` also its outline projected when
/// the sketch was made (a stored curve `c100` on the edge).
fn automatic(outline: bool) -> Vec<RegionCurve> {
    let edge = EdgeName::new(
        FaceName::new(FeatureUid(2), "end", None),
        FaceName::new(FeatureUid(2), "side", None),
    )
    .with_index(0);
    let mut face_edge = circle(0, [0.0, 0.0], 30.0);
    face_edge.uid = CurveId::FaceEdge(Box::new(edge.into()));
    let mut curves = vec![face_edge];
    if outline {
        curves.push(circle(100, [0.0, 0.0], 30.0));
    }
    curves
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3 * b.abs().max(1.0)
}

#[test]
fn a_circle_across_the_face_edge_is_the_outline_set() {
    for outline in [false, true] {
        let own = vec![circle(1, [30.0, 0.0], 10.0)];
        let found = own_area_regions(&own, &automatic(outline));
        assert!(
            found.layout.crossings().is_empty(),
            "{outline}: {:?}",
            found.layout.crossings()
        );
        let set = outline_set(&found.regions, &found.layout);
        assert!(!set.crossing, "{outline}");
        // The whole disc is in it.
        let disc = set
            .regions
            .iter()
            .map(|&i| &found.regions[i])
            .find(|r| r.profile.key.to_string() == "r{c1}")
            .unwrap_or_else(|| panic!("{outline}: no whole disc in {set:?}"));
        assert_eq!(disc.profile.loops.len(), 1);
        assert!(close(disc.area, PI * 100.0), "{outline}: {}", disc.area);
        // The face's leftover around it: bounded by the face's edge it is
        // not part of the set; bounded by the automatic outline, a closed
        // curve of the sketch, it is, as the outline alone is (mitcad#2194;
        // only the crossing changed in mitcad#2233).
        assert_eq!(found.regions.len(), 2, "{outline}: {:?}", found.regions);
        let leftover = found
            .regions
            .iter()
            .position(|r| r.profile.key.to_string() != "r{c1}")
            .unwrap();
        assert_eq!(
            found.regions[leftover].profile.face_bounded(),
            !outline,
            "{outline}"
        );
        assert_eq!(
            set.regions.contains(&leftover),
            outline,
            "{outline}: {set:?}"
        );
        assert_eq!(set.regions.len(), if outline { 2 } else { 1 });
    }
}

#[test]
fn two_overlapping_circles_on_a_face_still_cross() {
    for outline in [false, true] {
        // Inside the face, and across its edge.
        for center in [[0.0, 0.0], [30.0, 0.0]] {
            let own = vec![
                circle(1, center, 10.0),
                circle(2, [center[0], center[1] + 12.0], 10.0),
            ];
            let found = own_area_regions(&own, &automatic(outline));
            assert_eq!(
                found.layout.crossings().len(),
                2,
                "{outline} {center:?}: {:?}",
                found.layout.crossings()
            );
            let set = outline_set(&found.regions, &found.layout);
            assert!(set.crossing, "{outline} {center:?}");
            assert!(set.regions.is_empty(), "{outline} {center:?}: {set:?}");
        }
    }
}
