// SPDX-License-Identifier: MIT
//! The outline set of nested and crossing loops (mitcad#2194): a plate with
//! holes is the ring, an island in a hole is added, separate outlines are
//! each one, regions split by a line are all; two overlapping circles have
//! none.

use crate::geometry::Curve2;
use crate::ids::{EntityUid, FeatureUid};
use crate::outlines::outline_set;
use crate::regions::{Region, RegionCurve, face_bounded_regions, regions_and_layout};
use crate::topo::{CurveId, EdgeName, FaceName};

/// A rectangle of four lines (curves `first`..`first + 3`) from `lo` to `hi`.
fn rectangle(first: u32, lo: [f64; 2], hi: [f64; 2]) -> Vec<RegionCurve> {
    let corners = [lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]];
    (0..4)
        .map(|k| {
            RegionCurve::new(
                EntityUid(first + k as u32),
                Curve2::Line {
                    a: corners[k],
                    b: corners[(k + 1) % 4],
                },
            )
        })
        .collect()
}

fn circle(uid: u32, center: [f64; 2], radius: f64) -> RegionCurve {
    RegionCurve::new(
        EntityUid(uid),
        Curve2::Circle {
            center,
            radius,
            start: 0.0,
        },
    )
}

fn line(uid: u32, a: [f64; 2], b: [f64; 2]) -> RegionCurve {
    RegionCurve::new(EntityUid(uid), Curve2::Line { a, b })
}

/// The outline set's regions by their areas (rounded to 0.01 mm²), and
/// whether the curves cross.
fn outline_areas(curves: &[RegionCurve]) -> (Vec<f64>, bool, usize) {
    let (found, layout) = regions_and_layout(curves);
    let set = outline_set(&found, &layout);
    (areas(&found, &set.regions), set.crossing, found.len())
}

fn areas(found: &[Region], indices: &[usize]) -> Vec<f64> {
    let mut out: Vec<f64> = indices
        .iter()
        .map(|&i| (found[i].area * 100.0).round() / 100.0)
        .collect();
    out.sort_by(f64::total_cmp);
    out
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.5
}

#[test]
fn a_plate_with_two_holes_and_an_island_is_the_ring_and_the_island() {
    // A 100 x 60 plate, holes of radius 10 and 12, a 6 x 6 square inside
    // the first hole.
    let mut curves = rectangle(1, [0.0, 0.0], [100.0, 60.0]);
    curves.push(circle(5, [25.0, 30.0], 10.0));
    curves.push(circle(6, [70.0, 30.0], 12.0));
    curves.extend(rectangle(7, [22.0, 27.0], [28.0, 33.0]));
    let (set, crossing, count) = outline_areas(&curves);
    assert!(!crossing);
    // The plate, the ring in the first hole, the second hole's disc, the
    // island.
    assert_eq!(count, 4);
    let pi = std::f64::consts::PI;
    let ring = 6000.0 - pi * 100.0 - pi * 144.0;
    assert_eq!(set.len(), 2, "{set:?}");
    assert!(near(set[0], 36.0), "{set:?}");
    assert!(near(set[1], ring), "{set:?}");
}

#[test]
fn two_separate_rectangles_are_both() {
    let mut curves = rectangle(1, [0.0, 0.0], [10.0, 10.0]);
    curves.extend(rectangle(5, [20.0, 0.0], [40.0, 10.0]));
    let (set, crossing, _) = outline_areas(&curves);
    assert!(!crossing);
    assert_eq!(set.len(), 2);
    assert!(near(set[0], 100.0) && near(set[1], 200.0), "{set:?}");
}

#[test]
fn a_rectangle_split_by_a_line_is_both_halves() {
    let mut curves = rectangle(1, [0.0, 0.0], [40.0, 10.0]);
    // Past the sides: its ends bound nothing.
    curves.push(line(5, [10.0, -5.0], [10.0, 15.0]));
    let (set, crossing, count) = outline_areas(&curves);
    assert!(!crossing);
    assert_eq!(count, 2);
    assert_eq!(set.len(), 2);
    assert!(near(set[0], 100.0) && near(set[1], 300.0), "{set:?}");
}

#[test]
fn a_cross_inside_a_rectangle_divides_it() {
    // Two lines crossing inside the rectangle cross only inside its area:
    // the four quarters are the outline set.
    let mut curves = rectangle(1, [0.0, 0.0], [20.0, 20.0]);
    curves.push(line(5, [10.0, 0.0], [10.0, 20.0]));
    curves.push(line(6, [0.0, 10.0], [20.0, 10.0]));
    let (set, crossing, count) = outline_areas(&curves);
    assert!(!crossing);
    assert_eq!(count, 4);
    assert_eq!(set.len(), 4);
}

#[test]
fn two_overlapping_circles_cross_and_have_no_outline_set() {
    let curves = vec![circle(1, [0.0, 0.0], 10.0), circle(2, [12.0, 0.0], 10.0)];
    let (found, layout) = regions_and_layout(&curves);
    assert_eq!(found.len(), 3);
    assert_eq!(layout.crossings().len(), 2);
    let set = outline_set(&found, &layout);
    assert!(set.crossing);
    assert!(set.regions.is_empty());
}

#[test]
fn overlapping_circles_inside_a_plate_cross_too() {
    let mut curves = rectangle(1, [-50.0, -50.0], [50.0, 50.0]);
    curves.push(circle(5, [0.0, 0.0], 10.0));
    curves.push(circle(6, [12.0, 0.0], 10.0));
    let (_, crossing, _) = outline_areas(&curves);
    assert!(crossing);
}

#[test]
fn a_circle_across_a_rectangles_side_crosses() {
    let mut curves = rectangle(1, [0.0, 0.0], [40.0, 20.0]);
    curves.push(circle(5, [20.0, 20.0], 5.0));
    let (_, crossing, _) = outline_areas(&curves);
    assert!(crossing);
}

#[test]
fn touching_circles_do_not_cross() {
    // A circle inside another, touching it, and one outside touching it on
    // the other side.
    let curves = vec![
        circle(1, [0.0, 0.0], 10.0),
        circle(2, [5.0, 0.0], 5.0),
        circle(3, [-15.0, 0.0], 5.0),
    ];
    let (found, layout) = regions_and_layout(&curves);
    assert!(layout.crossings().is_empty());
    assert!(!outline_set(&found, &layout).crossing);
}

#[test]
fn concentric_circles_alternate() {
    // Radii 40, 30, 20, 10: the outer ring and the third ring are the set.
    let curves: Vec<RegionCurve> = (1..=4)
        .map(|k| circle(k, [0.0, 0.0], 50.0 - 10.0 * f64::from(k)))
        .collect();
    let (set, crossing, count) = outline_areas(&curves);
    assert!(!crossing);
    assert_eq!(count, 4);
    let pi = std::f64::consts::PI;
    assert_eq!(set.len(), 2, "{set:?}");
    assert!(near(set[0], pi * (400.0 - 100.0)), "{set:?}");
    assert!(near(set[1], pi * (1600.0 - 900.0)), "{set:?}");
}

#[test]
fn the_faces_leftover_region_is_not_part_of_it_and_its_holes_do_not_count() {
    // A circle drawn on a circular face of radius 30: the face's leftover
    // region around it has the drawn circle as its hole, yet the disc is
    // the outline set alone.
    let edge = EdgeName::new(
        FaceName::new(FeatureUid(2), "end", None),
        FaceName::new(FeatureUid(2), "side", None),
    )
    .with_index(0);
    let curves = vec![
        RegionCurve::new(
            CurveId::FaceEdge(Box::new(edge.into())),
            Curve2::Circle {
                center: [0.0, 0.0],
                radius: 30.0,
                start: 0.0,
            },
        ),
        circle(1, [5.0, 0.0], 8.0),
    ];
    let (found, layout) = face_bounded_regions(&curves);
    assert_eq!(found.len(), 2);
    let set = outline_set(&found, &layout);
    assert!(!set.crossing);
    assert_eq!(set.regions.len(), 1);
    assert!(!found[set.regions[0]].profile.face_bounded());
}
