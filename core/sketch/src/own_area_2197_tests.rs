// SPDX-License-Identifier: MIT
//! The face's own edges and its outline projected when the sketch was made
//! do not split an area the sketch's own curves close (mitcad#2197,
//! [`own_area_regions`]).

use std::f64::consts::{PI, TAU};

use crate::geometry::Curve2;
use crate::ids::{EntityUid, FeatureUid};
use crate::regions::{Region, RegionCurve, own_area_regions};
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

/// The edge `index` of the circular face of radius `radius` about the
/// origin (an outer edge, or a hole's).
fn face_edge(index: u32, radius: f64) -> RegionCurve {
    let edge = EdgeName::new(
        FaceName::new(FeatureUid(2), "end", None),
        FaceName::new(FeatureUid(2), "side", None),
    )
    .with_index(index);
    let mut curve = circle(0, [0.0, 0.0], radius);
    curve.uid = CurveId::FaceEdge(Box::new(edge.into()));
    curve
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

/// The automatic boundaries of a circular face of radius 30: its edge,
/// and with `outline` also its outline projected when the sketch was made
/// (a stored curve `c100` on the edge).
fn automatic(outline: bool) -> Vec<RegionCurve> {
    let mut curves = vec![face_edge(0, 30.0)];
    if outline {
        curves.push(circle(100, [0.0, 0.0], 30.0));
    }
    curves
}

fn areas(found: &[Region]) -> Vec<f64> {
    let mut list: Vec<f64> = found.iter().map(|r| r.area).collect();
    list.sort_by(f64::total_cmp);
    list
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3 * b.abs().max(1.0)
}

#[test]
fn a_circle_across_the_face_edge_is_one_disc() {
    for outline in [false, true] {
        let own = vec![circle(1, [30.0, 0.0], 10.0)];
        let found = own_area_regions(&own, &automatic(outline));
        let keys: Vec<String> = found
            .regions
            .iter()
            .map(|r| r.profile.key.to_string())
            .collect();
        // The whole disc, keyed by its circle alone, and the face's
        // leftover around it.
        assert_eq!(found.regions.len(), 2, "{outline}: {keys:?}");
        let disc = found
            .regions
            .iter()
            .find(|r| r.profile.key.to_string() == "r{c1}")
            .unwrap_or_else(|| panic!("{outline}: no whole disc in {keys:?}"));
        assert!(close(disc.area, PI * 100.0), "{}", disc.area);
        assert_eq!(disc.profile.loops.len(), 1);
        // The leftover is the face less the lens the disc covers of it
        // (circles of radii 10 and 30, centres 30 apart).
        let (r, big, d): (f64, f64, f64) = (10.0, 30.0, 30.0);
        let lens = r * r * ((d * d + r * r - big * big) / (2.0 * d * r)).acos()
            + big * big * ((d * d + big * big - r * r) / (2.0 * d * big)).acos()
            - 0.5 * ((-d + r + big) * (d + r - big) * (d - r + big) * (d + r + big)).sqrt();
        let outside = found
            .regions
            .iter()
            .find(|r| r.profile.key.to_string() != "r{c1}")
            .unwrap();
        assert!(
            close(outside.area, PI * 900.0 - lens),
            "{} against {}",
            outside.area,
            PI * 900.0 - lens
        );
        // The two halves the face's edge cut are kept for old references.
        assert_eq!(found.earlier.len(), 2, "{:?}", found.earlier);
        assert!(close(
            found.earlier.iter().map(|r| r.area).sum::<f64>(),
            PI * 100.0
        ));
        assert!(
            found
                .earlier
                .iter()
                .all(|r| r.profile.key.to_string().contains("c1"))
        );
    }
}

#[test]
fn a_rectangle_larger_than_the_face_is_one_plate_with_its_holes() {
    for outline in [false, true] {
        let mut own = rectangle(1, [-40.0, -40.0], [40.0, 40.0]);
        // One circle across the face's edge, one inside the face.
        own.push(circle(5, [30.0, 0.0], 5.0));
        own.push(circle(6, [0.0, 0.0], 5.0));
        let found = own_area_regions(&own, &automatic(outline));
        assert_eq!(found.regions.len(), 3, "{outline}: {:?}", found.regions);
        let plate = found
            .regions
            .iter()
            .find(|r| r.profile.loops.len() == 3)
            .unwrap_or_else(|| panic!("no plate with two holes: {:?}", found.regions));
        assert!(
            close(plate.area, 6400.0 - 2.0 * PI * 25.0),
            "{}",
            plate.area
        );
        assert!(!plate.profile.face_bounded());
        let mut discs: Vec<String> = found
            .regions
            .iter()
            .filter(|r| r.profile.loops.len() == 1)
            .map(|r| r.profile.key.to_string())
            .collect();
        discs.sort();
        assert_eq!(discs, ["r{c5}", "r{c6}"]);
        // No region of the face is left over: the plate covers it.
        assert!(found.regions.iter().all(|r| !r.profile.face_bounded()));
    }
}

#[test]
fn an_octagon_inside_a_circular_face_still_leaves_the_area_between() {
    for outline in [false, true] {
        let found = own_area_regions(&octagon(1, 20.0), &automatic(outline));
        assert_eq!(found.regions.len(), 2, "{outline}: {:?}", found.regions);
        let octagon_area = 8.0 * 400.0 * (PI / 8.0).tan();
        // Sorted by area: the octagon (1325.5) before the area between (1502.0).
        assert!(close(areas(&found.regions)[0], octagon_area));
        assert!(close(areas(&found.regions)[1], PI * 900.0 - octagon_area));
        assert!(found.earlier.is_empty());
    }
}

#[test]
fn a_line_across_the_face_still_splits_it() {
    for outline in [false, true] {
        let own = vec![RegionCurve::new(
            EntityUid(1),
            Curve2::Line {
                a: [-40.0, 0.0],
                b: [40.0, 0.0],
            },
        )];
        let found = own_area_regions(&own, &automatic(outline));
        assert_eq!(found.regions.len(), 2, "{outline}: {:?}", found.regions);
        for region in &found.regions {
            assert!(close(region.area, PI * 450.0), "{}", region.area);
        }
    }
}

#[test]
fn an_edge_the_user_projected_across_a_rectangle_still_splits_it() {
    // A drawn rectangle across the face, and a line the user projected
    // across it (an own curve, as a projection the user chose is).
    let mut own = rectangle(1, [-10.0, -10.0], [10.0, 10.0]);
    own.push(RegionCurve::new(
        EntityUid(5),
        Curve2::Line {
            a: [0.0, -20.0],
            b: [0.0, 20.0],
        },
    ));
    for outline in [false, true] {
        let found = own_area_regions(&own, &automatic(outline));
        // The two halves of the rectangle, and the face around it (bounded
        // by the face's edge, or by the outline on it).
        assert_eq!(found.regions.len(), 3, "{outline}: {:?}", found.regions);
        let halves: Vec<&Region> = found
            .regions
            .iter()
            .filter(|r| close(r.area, 200.0))
            .collect();
        assert_eq!(halves.len(), 2, "{outline}: {:?}", found.regions);
        assert!(
            halves
                .iter()
                .all(|r| r.profile.key.to_string().contains("c5"))
        );
        assert!(
            found
                .regions
                .iter()
                .any(|r| close(r.area, PI * 900.0 - 400.0))
        );
    }
}

#[test]
fn a_hole_of_the_face_inside_a_drawn_circle_makes_no_hole_in_it() {
    // A face of radius 30 with a hole of radius 10, and a drawn circle of
    // radius 20 about both.
    let own = vec![circle(1, [0.0, 0.0], 20.0)];
    let automatic = vec![face_edge(0, 30.0), face_edge(1, 10.0)];
    let found = own_area_regions(&own, &automatic);
    assert_eq!(found.regions.len(), 2, "{:?}", found.regions);
    let disc = found
        .regions
        .iter()
        .find(|r| r.profile.key.to_string() == "r{c1}")
        .unwrap();
    assert_eq!(disc.profile.loops.len(), 1);
    assert!(close(disc.area, PI * 400.0));
    // The face's leftover is the ring outside the circle.
    let ring = found
        .regions
        .iter()
        .find(|r| r.profile.key.to_string() != "r{c1}")
        .unwrap();
    assert!(close(ring.area, PI * 500.0), "{}", ring.area);
}

#[test]
fn a_sketch_without_own_areas_keeps_the_face_bounded_regions() {
    let found = own_area_regions(&[], &automatic(true));
    assert_eq!(found.regions.len(), 1);
    assert!(close(found.regions[0].area, PI * 900.0));
    assert!(found.earlier.is_empty());
}
