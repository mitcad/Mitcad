// SPDX-License-Identifier: MIT
//! Sketches: definitions, solving, regions and keys.

use std::f64::consts::PI;

use super::geometry::{Nurbs, clamped_uniform_knots};
use super::regions::{Region, RegionCurve, regions};
use super::solve::{DimensionInput, SolverItem, solve};
use super::*;
use crate::def::SketchDef;
use crate::parameters::ParamId;
use crate::topo::{RegionKey, SegmentKey};

#[test]
fn ids_and_refs_have_their_text_forms() {
    assert_eq!(ConstraintUid(3).to_string(), "k3");
    assert_eq!("k3".parse::<ConstraintUid>().unwrap(), ConstraintUid(3));
    assert_eq!("p4".parse::<Ref>().unwrap(), Ref::Point(EntityUid(4)));
    assert_eq!(Ref::Curve(EntityUid(5)).to_string(), "c5");
    assert!("x4".parse::<Ref>().is_err());
}

/// Builds sketches: curves get the ids 1, 2, ... in order; points the ids
/// from 101 (shared when at the same position).
#[derive(Default)]
struct Builder {
    def: SketchDef<ParamId>,
    next_curve: u32,
    next_point: u32,
}

fn c(n: u32) -> EntityUid {
    EntityUid(n)
}

impl Builder {
    fn new() -> Self {
        Self {
            next_point: 100,
            ..Self::default()
        }
    }

    fn point(&mut self, at: [f64; 2]) -> EntityUid {
        if let Some(e) = self
            .def
            .entities
            .iter()
            .find(|e| matches!(e.kind, EntityKind::Point { at: p } if p == at))
        {
            return e.id;
        }
        self.next_point += 1;
        let id = EntityUid(self.next_point);
        self.def.entities.push(Entity::point(id, at));
        id
    }

    fn curve(&mut self, kind: EntityKind) -> EntityUid {
        self.next_curve += 1;
        let id = EntityUid(self.next_curve);
        self.def.entities.push(Entity::new(id, kind));
        id
    }

    fn line(&mut self, a: [f64; 2], b: [f64; 2]) -> EntityUid {
        let (start, end) = (self.point(a), self.point(b));
        self.curve(EntityKind::Line {
            start,
            end,
            centerline: false,
        })
    }

    /// A closed polygon, a line per side.
    fn polygon(&mut self, corners: &[[f64; 2]]) -> Vec<EntityUid> {
        (0..corners.len())
            .map(|i| self.line(corners[i], corners[(i + 1) % corners.len()]))
            .collect()
    }

    fn rectangle(&mut self, x: f64, y: f64, w: f64, h: f64) -> Vec<EntityUid> {
        self.polygon(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]])
    }

    fn circle(&mut self, center: [f64; 2], radius: f64) -> EntityUid {
        let center = self.point(center);
        self.curve(EntityKind::Circle {
            center,
            radius,
            start: 0.0,
        })
    }

    fn arc(&mut self, center: [f64; 2], start: [f64; 2], end: [f64; 2]) -> EntityUid {
        let (center, start, end) = (self.point(center), self.point(start), self.point(end));
        self.curve(EntityKind::Arc { center, start, end })
    }

    fn solved(&self) -> solve::Solved {
        solve(&self.def.entities, &self.def.constraints, &[]).unwrap()
    }

    fn regions(&self) -> Vec<Region> {
        let mut def = SketchDef::<ParamId>::default();
        def.entities = self.def.entities.clone();
        def.regions_of(&self.solved(), &[])
    }
}

fn keys(regions: &[Region]) -> Vec<String> {
    regions.iter().map(|r| r.profile.key.to_string()).collect()
}

fn area(regions: &[Region], key: &str) -> f64 {
    regions
        .iter()
        .find(|r| r.profile.key.to_string() == key)
        .unwrap_or_else(|| panic!("no region {key} in {:?}", keys(regions)))
        .area
}

fn close(a: f64, b: f64, relative: f64) -> bool {
    (a - b).abs() <= relative * b.abs().max(1.0)
}

#[test]
fn a_rectangle_is_one_region_named_by_its_lines() {
    let mut b = Builder::new();
    b.rectangle(0.0, 0.0, 60.0, 40.0);
    let regions = b.regions();
    assert_eq!(
        keys(&regions),
        ["r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"]
    );
    assert!(close(regions[0].area, 2400.0, 1e-12));
    assert_eq!(regions[0].profile.loops.len(), 1);
    assert!(close(regions[0].centroid[0], 30.0, 1e-9) && close(regions[0].centroid[1], 20.0, 1e-9));
}

#[test]
fn overlapping_rectangles_make_three_regions() {
    // The reference model prof_overlap_region.
    let mut b = Builder::new();
    b.rectangle(0.0, 0.0, 20.0, 20.0); // c1..c4
    b.rectangle(10.0, 10.0, 20.0, 20.0); // c5..c8
    let regions = b.regions();
    assert_eq!(regions.len(), 3, "{:?}", keys(&regions));
    let mut areas: Vec<f64> = regions.iter().map(|r| r.area).collect();
    areas.sort_by(f64::total_cmp);
    for (a, expected) in areas.iter().zip([100.0, 300.0, 300.0]) {
        assert!(close(*a, expected, 1e-9), "{areas:?}");
    }
    // The middle square: pieces of c2, c3 (first) and c5, c8 (second),
    // each between the curves at its ends, in its own direction.
    assert!(
        keys(&regions).contains(&"r{c2[c5,c3],c3[c2,c8],c5[c8,c2],c8[c3,c5]}".to_owned()),
        "{:?}",
        keys(&regions)
    );
}

/// A plate with a rectangle across it whose top and bottom lie on the
/// plate's (imported sketches: .f3d designs merge the overlapping lines), the
/// inner rectangle's lower corners `off` below the plate's edge.
fn plate_with_band(off: f64) -> Vec<Region> {
    let mut b = Builder::new();
    b.rectangle(0.0, 0.0, 142.4465, 15.0); // c1..c4
    b.polygon(&[
        [55.1772, -off],
        [85.1777, -off],
        [85.1777, 15.0],
        [55.1772, 15.0],
    ]); // c5..c8
    b.regions()
}

#[test]
fn rectangles_on_collinear_edges_make_three_plain_regions() {
    for off in [0.0, 2.6e-7] {
        let regions = plate_with_band(off);
        let mut areas: Vec<f64> = regions.iter().map(|r| r.area).collect();
        areas.sort_by(f64::total_cmp);
        let expected = [450.0075, 827.658, 859.032];
        assert_eq!(areas.len(), 3, "off {off}: {:?}", keys(&regions));
        for (a, e) in areas.iter().zip(expected) {
            assert!(close(*a, e, 1e-6), "off {off}: {areas:?}");
        }
        // Each a single loop of four segments: the overlapping pieces count
        // once.
        for r in &regions {
            assert_eq!(r.profile.loops.len(), 1, "off {off}: {}", r.profile.key);
            assert_eq!(
                r.profile.loops[0].segments.len(),
                4,
                "off {off}: {}",
                r.profile.key
            );
        }
    }
}

#[test]
fn edges_a_little_apart_in_a_small_sketch_still_coincide() {
    // A small rectangle on the edge of a bigger one, the bigger one's edge
    // 1.9e-7 mm off (the stored solution of an imported sketch): the
    // notch is one region of one loop, not a region with a hole that
    // touches the outline.
    let mut b = Builder::new();
    b.rectangle(0.0, 0.0, 8.0, 13.0);
    b.polygon(&[
        [-7.0, -5.0],
        [15.0, -5.0],
        [15.0, 13.0 + 1.9e-7],
        [-7.0, 13.0 + 1.9e-7],
    ]);
    let regions = b.regions();
    let mut areas: Vec<f64> = regions.iter().map(|r| r.area).collect();
    areas.sort_by(f64::total_cmp);
    assert_eq!(areas.len(), 2, "{:?}", keys(&regions));
    assert!(close(areas[0], 104.0, 1e-6), "{areas:?}");
    assert!(close(areas[1], 292.0, 1e-6), "{areas:?}");
    for r in &regions {
        assert!(r.holes.is_empty(), "{}", r.profile.key);
        assert_eq!(r.profile.loops.len(), 1, "{}", r.profile.key);
    }
}

#[test]
fn overlapping_corner_shapes_tile_the_outline() {
    // An imported enclosure sketch: a rounded rectangle inside another,
    // full circles on two of its corner arcs and squares over those
    // corners whose sides lie partly on its edges.
    let mut b = Builder::new();
    let (r, big) = (1.5, 4.5);
    let corners = [
        ([12.4, 28.26], 0.0),
        ([-11.0, 28.26], 90.0),
        ([-11.0, -28.5], 180.0),
        ([12.4, -28.5], 270.0),
    ];
    for radius in [r, big] {
        let at = |c: [f64; 2], deg: f64| {
            let a = f64::to_radians(deg);
            [c[0] + radius * a.cos(), c[1] + radius * a.sin()]
        };
        for k in 0..4 {
            let (c, a0) = corners[k];
            let (next, b0) = corners[(k + 1) % 4];
            b.arc(c, at(c, a0), at(c, a0 + 90.0));
            b.line(at(c, a0 + 90.0), at(next, b0));
        }
    }
    b.line([-12.5, -21.182], [13.9, -21.182]);
    b.circle([12.4, 28.26], r);
    b.circle([-11.0, 28.26], r);
    b.polygon(&[[9.9, 29.76], [13.9, 29.76], [13.9, 25.76], [9.9, 25.76]]);
    b.polygon(&[[-8.5, 29.76], [-12.5, 29.76], [-12.5, 25.76], [-8.5, 25.76]]);
    let regions = b.regions();
    // The regions cover the outer rounded rectangle once.
    let outline = 32.4 * 65.76 - (4.0 - PI) * big * big;
    let total: f64 = regions.iter().map(|r| r.area).sum();
    assert!(
        close(total, outline, 2e-3),
        "{total} != {outline}: {:?}",
        keys(&regions)
    );
    // The ring between the rectangles: the outer one with the inner one
    // (and the squares' corners outside it) as its hole.
    let ring = regions
        .iter()
        .find(|r| r.profile.loops.len() == 2)
        .unwrap_or_else(|| panic!("no ring in {:?}", keys(&regions)));
    assert_eq!(ring.profile.loops[0].segments.len(), 8);
}

#[test]
fn nested_loops_make_holes_and_islands() {
    let mut b = Builder::new();
    b.rectangle(0.0, 0.0, 100.0, 100.0); // c1..c4
    b.circle([50.0, 50.0], 30.0); // c5
    b.circle([50.0, 50.0], 10.0); // c6
    let regions = b.regions();
    assert_eq!(
        keys(&regions),
        [
            "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}",
            "r{c5}",
            "r{c6}"
        ]
    );
    // The plate has the big circle as a hole, the ring the small one; the
    // small disk is an island.
    let loops: Vec<usize> = regions.iter().map(|r| r.profile.loops.len()).collect();
    assert_eq!(loops, [2, 2, 1]);
    // The holes come sampled too: the ring's is the small circle.
    let holes: Vec<usize> = regions.iter().map(|r| r.holes.len()).collect();
    assert_eq!(holes, [1, 1, 0]);
    assert!(
        regions[1].holes[0]
            .iter()
            .all(|p| ((p[0] - 50.0).hypot(p[1] - 50.0) - 10.0).abs() < 1e-6)
    );
    let rel = 2e-3; // sampled circles
    assert!(close(regions[0].area, 10_000.0 - PI * 900.0, rel));
    assert!(close(regions[1].area, PI * 800.0, rel));
    assert!(close(regions[2].area, PI * 100.0, rel));
    // A hole touching nothing else is a loop of its own segment.
    assert_eq!(
        regions[0].profile.loops[1].segments[0].key.to_string(),
        "c5"
    );
}

#[test]
fn dangling_curves_do_not_split_and_crossing_ones_do() {
    let mut b = Builder::new();
    b.rectangle(0.0, 0.0, 40.0, 20.0); // c1..c4
    b.line([10.0, 5.0], [20.0, 5.0]); // c5 inside, free
    b.line([30.0, 0.0], [30.0, 10.0]); // c6 from the bottom edge, free end
    let regions = b.regions();
    assert_eq!(regions.len(), 1, "{:?}", keys(&regions));
    // The bottom keeps one segment: the dangling c6 is gone.
    assert_eq!(
        keys(&regions),
        ["r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"]
    );
    // A line all the way across splits it in two.
    b.line([30.0, 10.0], [30.0, 20.0]); // c7 continues c6 to the top
    let regions = b.regions();
    assert_eq!(regions.len(), 2, "{:?}", keys(&regions));
    assert!(close(
        area(&regions, &keys(&regions)[0]) + area(&regions, &keys(&regions)[1]),
        800.0,
        1e-9
    ));
    // c6 and c7 meet at a point no other curve has: the keys say so.
    assert!(
        keys(&regions)
            .iter()
            .any(|k| k.contains("c6[c1,c7]") && k.contains("c7[c6,c3]")),
        "{:?}",
        keys(&regions)
    );
}

#[test]
fn several_curves_at_a_point_are_joined_with_plus() {
    // A rectangle with both diagonals: four triangles, the diagonals
    // crossing in the middle.
    let mut b = Builder::new();
    b.rectangle(0.0, 0.0, 20.0, 10.0); // c1..c4
    b.line([0.0, 0.0], [20.0, 10.0]); // c5
    b.line([20.0, 0.0], [0.0, 10.0]); // c6
    let regions = b.regions();
    assert_eq!(regions.len(), 4, "{:?}", keys(&regions));
    for r in &regions {
        assert!(close(r.area, 50.0, 1e-9), "{:?}", keys(&regions));
    }
    // The bottom triangle: c1 between the corners where c4+c5 and c2+c6
    // meet it, and the diagonal halves.
    assert!(
        keys(&regions).contains(&"r{c1[c4+c5,c2+c6],c5[c1+c4,c6]#0,c6[c1+c2,c5]#0}".to_owned())
            || keys(&regions)
                .iter()
                .any(|k| k.starts_with("r{c1[c4+c5,c2+c6]")),
        "{:?}",
        keys(&regions)
    );
}

#[test]
fn touching_circles_split_at_the_contact_point() {
    // Outside each other.
    let mut b = Builder::new();
    b.circle([0.0, 0.0], 10.0); // c1
    b.circle([15.0, 0.0], 5.0); // c2
    let regions = b.regions();
    assert_eq!(keys(&regions), ["r{c1[c2,c2]}", "r{c2[c1,c1]}"]);
    assert!(close(area(&regions, "r{c2[c1,c1]}"), PI * 25.0, 2e-3));
    // One inside the other, touching it: the ring is open at the point.
    let mut b = Builder::new();
    b.circle([0.0, 0.0], 10.0); // c1
    b.circle([5.0, 0.0], 5.0); // c2
    let regions = b.regions();
    assert_eq!(keys(&regions), ["r{c1[c2,c2]}", "r{c2[c1,c1]}"]);
    assert!(close(area(&regions, "r{c1[c2,c2]}"), PI * 75.0, 2e-3));
    assert_eq!(
        regions[0].profile.loops.len(),
        2,
        "the inner circle is a hole"
    );
}

#[test]
fn a_circle_drawn_twice_bounds_its_regions_once() {
    // A ring and a disc in it whose circle is there twice (imported
    // sketches), each copy a loop by itself: one hole of the ring and one
    // disc (mitcad#93), the lower id kept.
    let mut b = Builder::new();
    b.circle([0.0, 0.0], 4.000000000000003); // c1
    b.circle([0.0, 0.0], 6.1); // c2
    b.circle([0.0, 0.0], 4.000000000000003); // c3, on c1
    b.circle([0.0, 0.0], 6.0); // c4
    let regions = b.regions();
    assert_eq!(keys(&regions), ["r{c1}", "r{c2}", "r{c4}"]);
    let holes: Vec<usize> = regions.iter().map(|r| r.profile.loops.len() - 1).collect();
    assert_eq!(holes, [0, 1, 1]);
    assert!(close(area(&regions, "r{c1}"), PI * 16.0, 2e-3));
    assert!(close(area(&regions, "r{c4}"), PI * 20.0, 2e-3));
    // Copies within the tolerance of overlapping pieces too, also of
    // other closed curves; circles of other radii stay.
    let mut b = Builder::new();
    b.circle([0.0, 0.0], 10.0); // c1
    b.circle([1e-10, 0.0], 3.0); // c2
    b.circle([0.0, 0.0], 3.0); // c3
    b.circle([0.0, 0.0], 3.0 + 1e-4); // c4
    let regions = b.regions();
    assert_eq!(keys(&regions), ["r{c1}", "r{c2}", "r{c4}"]);
}

#[test]
fn a_circle_starts_where_its_start_angle_says() {
    use crate::profile::SegmentGeometry;
    // A circle drawn here starts on the x axis and its JSON has no angle.
    let mut b = Builder::new();
    b.circle([1.0, 2.0], 3.0);
    let region = &b.regions()[0];
    let geometry = &region.profile.loops[0].segments[0].geometry;
    assert!(matches!(geometry, SegmentGeometry::Circle { start, .. } if *start == 0.0));
    let json = serde_json::to_value(&b.def.entities[1].kind).unwrap();
    assert!(json.get("start_angle").is_none(), "{json}");
    // One an import started elsewhere (a quarter turn) reaches the profile
    // and the file, and its region is the same.
    let mut b = Builder::new();
    let center = b.point([1.0, 2.0]);
    b.curve(EntityKind::Circle {
        center,
        radius: 3.0,
        start: PI / 2.0,
    });
    let region = &b.regions()[0];
    let geometry = &region.profile.loops[0].segments[0].geometry;
    assert!(matches!(geometry, SegmentGeometry::Circle { start, .. } if *start == PI / 2.0));
    assert!(close(region.area, PI * 9.0, 1e-9));
    let json = serde_json::to_value(&b.def.entities[1].kind).unwrap();
    assert_eq!(json["start_angle"], PI / 2.0);
    let back: EntityKind = serde_json::from_value(json).unwrap();
    assert_eq!(back, b.def.entities[1].kind);
}

#[test]
fn a_tangent_line_does_not_cut_a_circle_into_regions() {
    let mut b = Builder::new();
    b.circle([0.0, 0.0], 10.0); // c1
    b.line([-20.0, 10.0], [20.0, 10.0]); // c2 touching the top
    let regions = b.regions();
    assert_eq!(keys(&regions), ["r{c1}"]);
    // A line through it makes two.
    b.line([-20.0, 0.0], [20.0, 0.0]); // c3
    let regions = b.regions();
    assert_eq!(regions.len(), 2, "{:?}", keys(&regions));
    assert!(close(regions[0].area, PI * 50.0, 2e-3));
    assert!(keys(&regions).iter().all(|k| k.contains("c3[c1,c1]")));
}

#[test]
fn a_grid_of_lines_makes_its_cells() {
    // Five horizontal and five vertical lines sticking out of each other.
    let mut b = Builder::new();
    for i in 0..5 {
        let v = i as f64 * 10.0;
        b.line([-5.0, v], [45.0, v]);
        b.line([v, -5.0], [v, 45.0]);
    }
    let regions = b.regions();
    assert_eq!(regions.len(), 16);
    assert!(regions.iter().all(|r| close(r.area, 100.0, 1e-9)));
    // Every key is distinct and the same on a second computation.
    let mut again = keys(&b.regions());
    again.dedup();
    assert_eq!(again.len(), 16);
    assert_eq!(again, keys(&regions));
}

#[test]
fn arcs_ellipses_and_splines_bound_regions() {
    // A half disk: an arc and its chord.
    let mut b = Builder::new();
    b.arc([0.0, 0.0], [10.0, 0.0], [-10.0, 0.0]); // c1, counter-clockwise over the top
    b.line([-10.0, 0.0], [10.0, 0.0]); // c2
    let regions = b.regions();
    assert_eq!(keys(&regions), ["r{c1[c2,c2],c2[c1,c1]}"]);
    assert!(close(regions[0].area, PI * 50.0, 2e-3));

    // An ellipse cut by a line through its centre.
    let mut b = Builder::new();
    let center = b.point([0.0, 0.0]);
    let major = b.point([20.0, 0.0]);
    b.curve(EntityKind::Ellipse {
        center,
        major,
        minor_radius: 10.0,
    }); // c1
    b.line([0.0, -15.0], [0.0, 15.0]); // c2
    let regions = b.regions();
    assert_eq!(regions.len(), 2, "{:?}", keys(&regions));
    for r in &regions {
        assert!(close(r.area, PI * 100.0, 2e-3), "{}", r.area);
    }

    // A spline over a line: the region between them.
    let mut b = Builder::new();
    let control = [[0.0, 0.0], [10.0, 20.0], [30.0, 20.0], [40.0, 0.0]].map(|p| b.point(p));
    b.curve(EntityKind::Spline {
        degree: 3,
        control: control.to_vec(),
        weights: Vec::new(),
        knots: Vec::new(),
    }); // c1
    b.line([0.0, 0.0], [40.0, 0.0]); // c2
    let regions = b.regions();
    assert_eq!(keys(&regions), ["r{c1[c2,c2],c2[c1,c1]}"]);
    // A Bezier's area under it: integral of y dx over the curve.
    let spline = Nurbs {
        degree: 3,
        control: vec![[0.0, 0.0], [10.0, 20.0], [30.0, 20.0], [40.0, 0.0]],
        weights: Vec::new(),
        knots: clamped_uniform_knots(4, 3),
    };
    let n = 20_000;
    let exact: f64 = (0..n)
        .map(|i| {
            let t = (i as f64 + 0.5) / n as f64;
            let [p, d, _] = spline.eval(t);
            p[1] * d[0] / n as f64
        })
        .sum();
    assert!(
        close(regions[0].area, exact, 1e-3),
        "{} {exact}",
        regions[0].area
    );

    // A wavy spline crossing a line several times makes several regions.
    let mut b = Builder::new();
    let control: Vec<EntityUid> = (0..8)
        .map(|i| b.point([i as f64 * 10.0, if i % 2 == 0 { -10.0 } else { 10.0 }]))
        .collect();
    b.curve(EntityKind::Spline {
        degree: 3,
        control,
        weights: Vec::new(),
        knots: Vec::new(),
    }); // c1
    b.line([0.0, 0.0], [70.0, 0.0]); // c2
    let regions = b.regions();
    assert!(regions.len() >= 4, "{:?}", keys(&regions));
    // Pieces of the spline between the same curves are numbered along it.
    assert!(keys(&regions).iter().any(|k| k.contains("c1[c2,c2]#1")));
}

#[test]
fn keys_do_not_depend_on_order_or_sizes() {
    let build = |w: f64, reversed: bool| {
        let mut b = Builder::new();
        if reversed {
            b.next_point = 200;
        }
        b.rectangle(0.0, 0.0, w, 20.0);
        b.circle([w / 2.0, 10.0], 5.0);
        let mut entities = b.def.entities.clone();
        if reversed {
            entities.reverse();
        }
        let curves: Vec<RegionCurve> = {
            let solved = solve(&entities, &[], &[]).unwrap();
            entities
                .iter()
                .filter(|e| !e.is_point())
                .map(|e| RegionCurve::new(e.id, solved.curves[&e.id].clone()))
                .collect()
        };
        keys(&regions(&curves))
    };
    let base = build(40.0, false);
    assert_eq!(build(80.0, false), base);
    assert_eq!(build(40.0, true), base);
    assert_eq!(
        base,
        ["r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}", "r{c5}"]
    );
}

#[test]
fn changed_sketches_find_the_closest_region() {
    use crate::def::SketchOutput;
    let mut b = Builder::new();
    b.rectangle(0.0, 0.0, 40.0, 20.0); // c1..c4
    let whole: RegionKey = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"
        .parse()
        .unwrap();
    b.line([10.0, 0.0], [10.0, 20.0]); // c5 splits off a strip on the left
    let solved = b.solved();
    let info = b.regions();
    let output = SketchOutput {
        frame: crate::profile::SketchFrame::XY,
        base: crate::profile::SketchFrame::XY,
        regions: info.iter().map(|r| r.profile.clone()).collect(),
        region_info: info,
        layout: Default::default(),
        solved: std::sync::Arc::new(solved),
        texts: Default::default(),
        moved: Default::default(),
        follow: Default::default(),
    };
    // Both pieces share three curves with the old key; the bigger one wins.
    // It carries the old key, so the faces made from it keep their names.
    let found = output.resolve_region(&whole).unwrap();
    assert_eq!(found.key, whole);
    let outer = &found.loops[0].segments;
    assert!(
        outer.iter().any(|s| s.key.curve == c(2).into()),
        "the right part"
    );
    assert!(output.resolve_region(&"r{c9}".parse().unwrap()).is_none());
}

#[test]
fn segment_keys_parse_back() {
    let key = SegmentKey::with_ends(c(3), vec![c(5).into(), c(2).into()], vec![]);
    assert_eq!(key.to_string(), "c3[c2+c5,-]");
    assert_eq!(key.to_string().parse::<SegmentKey>().unwrap(), key);
}

// Solving.

fn fixed(mut e: Entity) -> Entity {
    e.fixed = true;
    e
}

#[test]
fn the_solver_reports_freedom_conflicts_and_driven_values() {
    // A line from a fixed point, horizontal, 10 long: fully constrained.
    let entities = vec![
        fixed(Entity::point(c(1), [0.0, 0.0])),
        Entity::point(c(2), [9.0, 0.5]),
        Entity::new(
            c(3),
            EntityKind::Line {
                start: c(1),
                end: c(2),
                centerline: false,
            },
        ),
    ];
    let horizontal = Constraint::new(ConstraintUid(1), ConstraintKind::Horizontal { line: c(3) });
    let length = DimensionKind::Length { line: c(3) };
    let driving = [DimensionInput {
        id: ConstraintUid(2),
        kind: &length,
        value: Some(10.0),
    }];
    let solved = solve(&entities, std::slice::from_ref(&horizontal), &driving).unwrap();
    let end = solved.points[&c(2)];
    assert!(
        (end[0] - 10.0).abs() < 1e-9 && end[1].abs() < 1e-9,
        "{end:?}"
    );
    assert_eq!(solved.status.dof, 0);
    assert!(solved.status.fully_constrained.contains(&c(3)));
    // Free, it has two degrees of freedom left and measures its length.
    let driven = [DimensionInput {
        id: ConstraintUid(2),
        kind: &length,
        value: None,
    }];
    let solved = solve(&entities, std::slice::from_ref(&horizontal), &driven).unwrap();
    assert_eq!(solved.status.dof, 1);
    assert!((solved.status.driven[&ConstraintUid(2)] - 9.0).abs() < 1e-9);
    // Vertical as well: a conflict naming both constraints.
    let vertical = Constraint::new(ConstraintUid(3), ConstraintKind::Vertical { line: c(3) });
    let error = solve(&entities, &[horizontal, vertical], &driving).unwrap_err();
    assert!(
        error.message.contains("k1") && error.message.contains("k3"),
        "{}",
        error.message
    );
    assert!(
        error
            .status
            .conflicts
            .iter()
            .flatten()
            .any(|i| *i == SolverItem::Constraint(ConstraintUid(3)))
    );
}

#[test]
fn fixed_curves_that_miss_each_other_by_rounding_still_solve() {
    // Two fixed elliptical arcs that share a point; the second ellipse
    // passes 3e-6 mm from it (a linked curve's stored numbers are rounded).
    let mut b = Builder::new();
    let ellipse =
        |b: &mut Builder, center: [f64; 2], minor: f64, start: [f64; 2], end: [f64; 2]| {
            let (c, m) = (b.point(center), b.point([center[0] + 20.0, center[1]]));
            let (s, e) = (b.point(start), b.point(end));
            b.curve(EntityKind::EllipticalArc {
                center: c,
                major: m,
                minor_radius: minor,
                start: s,
                end: e,
            })
        };
    ellipse(&mut b, [0.0, 0.0], 10.0, [20.0, 0.0], [0.0, 10.0]);
    ellipse(&mut b, [0.0, 20.0], 10.000_003, [0.0, 10.0], [20.0, 20.0]);
    for e in &mut b.def.entities {
        e.fixed = true;
    }
    let solved = solve(&b.def.entities, &b.def.constraints, &[]);
    assert!(solved.is_ok(), "{:?}", solved.err().map(|e| e.message));
}

/// An imported sketch: a rectangle on a plate's top line and a triangle
/// whose long side nearly lies on the rectangle's top (4e-8 mm off): the
/// triangle outside the rectangle is two regions, the parts either side of
/// it, which the top edge does not join.
#[test]
fn a_near_collinear_triangle_side_does_not_join_the_parts_either_side() {
    let mut b = Builder::new();
    b.rectangle(37.000000551342964, 0.0, 2.000000007, 3.0000000000000004);
    b.line(
        [40.000002961231466, 1.0000000148970964],
        [8.482701646710629, 3.0000000000000004],
    );
    b.line(
        [8.482701646710629, 3.0000000000000004],
        [8.615016740606665, 2.991603653397221],
    );
    b.line(
        [40.00000059604645, 3.0000000447034836],
        [40.000002961231466, 1.0000000148970964],
    );
    b.line(
        [40.00000059604645, 3.0000000447034836],
        [8.482701646710629, 3.0000000000000004],
    );
    let regions = b.regions();
    let mut areas: Vec<f64> = regions.iter().map(|r| r.area).collect();
    areas.sort_by(f64::total_cmp);
    assert!(
        areas.iter().any(|a| close(*a, 25.80, 1e-3))
            && areas.iter().any(|a| close(*a, 1.968, 1e-3)),
        "{areas:?}: {:?}",
        keys(&regions)
    );
}

#[test]
fn a_line_that_ends_tangent_to_arcs_bounds_a_region_with_them() {
    // A concave arc (c1) closed on top by a line (c4) that ends at the top
    // of two small arcs (c2, c3) that continue past it: each arc is cut where
    // the line touches it and the lens stands between the line and the arc.
    let mut b = Builder::new();
    b.arc(
        [0.0, 73.0],
        [-6.217391304347393, 63.92563801864726],
        [6.21739130434783, 63.92563801864755],
    ); // c1
    b.arc(
        [6.5, 63.5131670194945],
        [7.0, 63.51316701949448],
        [6.217391304348286, 63.92563801864689],
    ); // c2
    b.arc(
        [-6.5, 63.5131670194945],
        [-6.217391304347392, 63.92563801864726],
        [-7.0, 63.51316701949447],
    ); // c3
    b.line([-6.5, 64.01316701949488], [6.5, 64.01316701949448]); // c4
    let regions = b.regions();
    assert_eq!(regions.len(), 1, "{:?}", keys(&regions));
    assert!(close(regions[0].area, 17.37, 1e-2), "{}", regions[0].area);
}

#[test]
fn a_spline_that_follows_a_curve_within_the_tolerance_bounds_no_sliver() {
    // A line and a spline between the same two points that stays within a
    // micron of it (a projected edge fitted by a spline): the two are one
    // edge, though the spline's parameter puts its middle elsewhere.
    let spline_over_line = |bow: f64| {
        let mut b = Builder::new();
        let control = [[0.0, 0.0], [1.0, bow], [5.0, bow], [10.0, 0.0]].map(|p| b.point(p));
        b.line([0.0, 0.0], [10.0, 0.0]); // c1
        b.curve(EntityKind::Spline {
            degree: 3,
            control: control.to_vec(),
            weights: Vec::new(),
            knots: Vec::new(),
        }); // c2
        b.regions()
    };
    assert!(keys(&spline_over_line(4e-4)).is_empty());
    // Further apart they bound a region.
    assert_eq!(spline_over_line(4e-2).len(), 1);
}

#[test]
fn of_a_spline_and_a_line_along_each_other_the_spline_bounds_the_region() {
    let mut b = Builder::new();
    let control = [[0.0, 0.0], [1.0, 4e-4], [5.0, 4e-4], [10.0, 0.0]].map(|p| b.point(p));
    b.line([0.0, 0.0], [10.0, 0.0]); // c1
    b.curve(EntityKind::Spline {
        degree: 3,
        control: control.to_vec(),
        weights: Vec::new(),
        knots: Vec::new(),
    }); // c2
    b.line([0.0, 0.0], [0.0, -5.0]); // c3
    b.line([0.0, -5.0], [10.0, -5.0]); // c4
    b.line([10.0, -5.0], [10.0, 0.0]); // c5
    let keys = keys(&b.regions());
    assert_eq!(keys.len(), 1, "{keys:?}");
    assert!(
        keys[0].contains("c2[") && !keys[0].contains("c1["),
        "{keys:?}"
    );
}

#[test]
fn an_angle_dimensions_side_is_the_sector_of_a_point() {
    use super::geometry::{angle_on_side, angle_side_at, angle_side_for};
    // A trapezoid's lower left corner (mitcad#296): the bottom from the
    // corner to the right, the side from up right down to the corner.
    let bottom = ([0.0, 0.0], [40.0, 0.0]);
    let side = ([10.0, 20.0], [0.0, 0.0]);
    let inner = 2.0f64.atan();
    // Inside the corner: the bottom's direction and the side's reversed.
    assert_eq!(angle_side_at(bottom, side, [5.0, 3.0]), Some([false, true]));
    assert!((angle_on_side(bottom, side, [false, true]) - inner).abs() < 1e-12);
    // The other three sectors.
    assert_eq!(angle_side_at(bottom, side, [-5.0, 3.0]), Some([true, true]));
    assert_eq!(
        angle_side_at(bottom, side, [-5.0, -3.0]),
        Some([true, false])
    );
    assert_eq!(
        angle_side_at(bottom, side, [5.0, -3.0]),
        Some([false, false])
    );
    assert!((angle_on_side(bottom, side, [false, false]) - (PI - inner)).abs() < 1e-12);
    assert_eq!(
        angle_side_at(bottom, ([0.0, 1.0], [9.0, 1.0]), [1.0, 1.0]),
        None
    );
    // A value and where it is shown: the sector there when its angle is
    // the value, else the neighbour nearer to it.
    assert_eq!(
        angle_side_for(bottom, side, inner, Some([5.0, 3.0]), 1e-6),
        Some([false, true])
    );
    assert_eq!(
        angle_side_for(bottom, side, PI - inner, Some([5.0, 8.0]), 1e-6),
        Some([true, true])
    );
    assert_eq!(
        angle_side_for(bottom, side, PI - inner, None, 1e-6),
        Some([false, false])
    );
    assert_eq!(
        angle_side_for(bottom, side, 0.5, Some([5.0, 3.0]), 1e-6),
        None
    );
    // Stored without a side (before mitcad#296): none, and none written.
    let stored: DimensionKind =
        serde_json::from_value(serde_json::json!({"type": "angle", "a": "c1", "b": "c2"})).unwrap();
    assert_eq!(stored.angle_side(), None);
    assert_eq!(
        serde_json::to_value(&stored).unwrap(),
        serde_json::json!({"type": "angle", "a": "c1", "b": "c2"})
    );
    let given: DimensionKind = serde_json::from_value(
        serde_json::json!({"type": "angle", "a": "c1", "b": "c2", "reverse_b": true}),
    )
    .unwrap();
    assert_eq!(given.angle_side(), Some([false, true]));
}

// --- Holes near curved boundaries (mitcad#364)

#[test]
fn a_hole_within_the_chord_error_of_a_circle_is_still_a_hole() {
    // The inner circle lies wholly inside the outer one, 0.011 mm from it
    // (less than the 9.5e-4 r the 5 degree chords of the outer circle
    // leave), and does not touch it.
    let mut b = Builder::new();
    b.circle([0.0, 0.0], 50.0); // c1
    b.circle([39.9669, -1.3083], 10.0); // c2
    let regions = b.regions();
    assert_eq!(keys(&regions), ["r{c1}", "r{c2}"]);
    let plate = &regions[0];
    assert_eq!(plate.profile.loops.len(), 2, "{:?}", keys(&regions));
    assert_eq!(plate.holes.len(), 1);
    assert!(close(plate.area, PI * 2400.0, 2e-3), "{}", plate.area);
    assert!(regions[1].holes.is_empty());
}

#[test]
fn a_loop_just_outside_a_concave_arc_is_no_hole() {
    // A face with a concave left side: an arc of a circle of radius 200
    // around (-250, 0), bulging into it. A small circle just inside that
    // big circle (0.02 mm from the arc, at the middle of a sample segment
    // of the arc) lies outside the face, but inside the face's sampled
    // outline.
    let r: f64 = 200.0;
    let centre = [-250.0, 0.0];
    let corner = (r * r - 50.0 * 50.0).sqrt() + centre[0];
    let mut b = Builder::new();
    b.line([corner, -50.0], [50.0, -50.0]); // c1
    b.line([50.0, -50.0], [50.0, 50.0]); // c2
    b.line([50.0, 50.0], [corner, 50.0]); // c3
    b.arc(centre, [corner, -50.0], [corner, 50.0]); // c4
    // The arc is split in six: the middle of a piece is at 2.413 degrees.
    let (sin, cos) = 2.413f64.to_radians().sin_cos();
    let (small, gap) = (5.0, 0.02);
    let from = r - small - gap;
    b.circle([centre[0] + from * cos, centre[1] + from * sin], small); // c5
    let regions = b.regions();
    let face = regions
        .iter()
        .find(|r| r.profile.loops[0].segments.len() == 4)
        .unwrap_or_else(|| panic!("no face in {:?}", keys(&regions)));
    assert_eq!(face.profile.loops.len(), 1, "{:?}", keys(&regions));
    assert!(face.holes.is_empty());
}

/// Seeding an offset whose line meets an arc at a corner that is not
/// tangent (mitcad#641): the shared point takes where the offset line
/// crosses the offset circle, not the middle of their ends; once there,
/// a second seed moves nothing, so an imported offset already in place
/// keeps the file's positions.
#[test]
fn a_seeded_offset_corner_of_a_line_and_an_arc_lies_on_both() {
    use super::offset::{SketchOffset, seed};
    let center = [5.0, 3.0];
    let r = (25.0f64 + 9.0).sqrt();
    let mut b = Builder::new();
    let line = b.line([30.0, 0.0], [0.0, 0.0]);
    let top = [center[0], center[1] + r];
    let arc = b.arc(center, top, [0.0, 0.0]);
    // The results drawn roughly a millimetre off their sources.
    let result_line = b.line([30.0, 1.3], [0.4, 1.2]);
    let arc_top = [center[0], center[1] + r - 0.8];
    let result_arc = b.arc(center, arc_top, [0.4, 1.2]);
    let offsets = vec![SketchOffset {
        id: ConstraintUid(1),
        curves: vec![line, arc],
        distance: ParamId::from_raw(0),
        left: false,
        results: vec![result_line, result_arc],
        corners: Vec::new(),
        helpers: Vec::new(),
        derived: false,
    }];
    let at = |entities: &[Entity], id: EntityUid| match entities
        .iter()
        .find(|e| e.id == id)
        .map(|e| &e.kind)
    {
        Some(EntityKind::Point { at }) => *at,
        _ => panic!("no point {id:?}"),
    };
    let ends = |entities: &[Entity], id: EntityUid| match entities
        .iter()
        .find(|e| e.id == id)
        .map(|e| &e.kind)
    {
        Some(EntityKind::Line { start, end, .. }) => (*start, *end),
        Some(EntityKind::Arc { start, end, .. }) => (*start, *end),
        _ => panic!("no curve {id:?}"),
    };
    let mut entities = b.def.entities.clone();
    seed(&offsets, &mut entities, &mut |_| Ok(1.0));
    let (a, corner) = ends(&entities, result_line);
    let (arc_start, arc_end) = ends(&entities, result_arc);
    assert_eq!(corner, arc_end);
    let y = at(&entities, a)[1];
    assert!((y.abs() - 1.0).abs() < 1e-12, "{y}");
    assert!((at(&entities, corner)[1] - y).abs() < 1e-12);
    // Towards the centre the circle shrinks, away from it it grows.
    let radius = if y > 0.0 { r - 1.0 } else { r + 1.0 };
    let from_center =
        |p: [f64; 2]| ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2)).sqrt();
    assert!((from_center(at(&entities, corner)) - radius).abs() < 1e-12);
    // The arc's free end at its source's angle (straight up).
    let free = at(&entities, arc_start);
    assert!((free[0] - center[0]).abs() < 1e-12 && (free[1] - center[1] - radius).abs() < 1e-12);
    let once = entities.clone();
    seed(&offsets, &mut entities, &mut |_| Ok(1.0));
    for (e, f) in once.iter().zip(&entities) {
        if let (EntityKind::Point { at: p }, EntityKind::Point { at: q }) = (&e.kind, &f.kind) {
            assert!(
                (p[0] - q[0]).abs() < 1e-12 && (p[1] - q[1]).abs() < 1e-12,
                "{:?}",
                e.id
            );
        }
    }
}

/// Seeding an offset whose line goes on tangentially into an arc
/// (mitcad#641): the shared point takes where the offset line touches the
/// offset circle. Their crossings lie the square root of the rounding away
/// from it (0.4 µm for 1e-15 mm), which had moved a grid design's tangent
/// corners enough to lose the edges its chamfers name.
#[test]
fn a_seeded_tangent_offset_corner_lies_where_the_line_touches_the_arc() {
    use super::offset::{SketchOffset, seed};
    let center = [0.0, 12.2];
    let r = 12.2;
    let mut b = Builder::new();
    let line = b.line([60.0, 0.0], [0.0, 0.0]);
    let arc = b.arc(center, [-r, 12.2], [0.0, 0.0]);
    // The results drawn a little off their sources.
    let result_line = b.line([60.0, 1.7], [0.1, 1.9]);
    let result_arc = b.arc(center, [-r + 2.1, 12.2], [0.1, 1.9]);
    let offsets = vec![SketchOffset {
        id: ConstraintUid(1),
        curves: vec![line, arc],
        distance: ParamId::from_raw(0),
        left: false,
        results: vec![result_line, result_arc],
        corners: Vec::new(),
        helpers: Vec::new(),
        derived: false,
    }];
    let point = |entities: &[Entity], id: EntityUid| match entities
        .iter()
        .find(|e| e.id == id)
        .map(|e| &e.kind)
    {
        Some(EntityKind::Point { at }) => *at,
        _ => panic!("no point {id:?}"),
    };
    let corner = |entities: &[Entity]| match entities
        .iter()
        .find(|e| e.id == result_line)
        .map(|e| &e.kind)
    {
        Some(EntityKind::Line { end, .. }) => point(entities, *end),
        _ => panic!("no line"),
    };
    let mut entities = b.def.entities.clone();
    seed(&offsets, &mut entities, &mut |_| Ok(2.0));
    let at = corner(&entities);
    assert_eq!(at[0], 0.0, "{at:?}");
    assert!((at[1].abs() - 2.0).abs() < 1e-12, "{at:?}");
}

/// Seeding the closed offset of a projected outline whose right end is one
/// circle split into two arcs, each with its own centre point lying within
/// rounding of the other's (mitcad#709): the corner the two offset arcs
/// share stays on their circle, where the old seed (the middle of their
/// ends) put it. Taken as circles touching inside each other, it had gone
/// to a "touching point" near the centre, and the solve started from there
/// lost the extrusion's profile. The chain runs as a projection gives it:
/// an arc, a line, an arc, a line against the chain, an arc, a line against
/// the chain and an arc, every join tangent, offset 3 mm outwards with the
/// results drawn in place and helpers across the arcs' gaps.
#[test]
fn a_seeded_corner_of_two_arcs_of_one_circle_stays_on_the_circle() {
    use super::offset::{SketchOffset, seed};
    // The split circle's two centres, a rounding apart.
    let (upper, lower) = ([50.0, 9e-16], [50.0, -7e-16]);
    let (left_top, left_bottom) = ([48.0, 1.0], [48.0, -1.0]);
    let mut b = Builder::new();
    let curves = vec![
        b.arc(upper, [56.0, 0.0], [50.0, 6.0]),
        b.line([50.0, 6.0], [48.0, 6.0]),
        b.arc(left_top, [48.0, 6.0], [43.0, 1.0]),
        b.line([43.0, -1.0], [43.0, 1.0]),
        b.arc(left_bottom, [43.0, -1.0], [48.0, -6.0]),
        b.line([50.0, -6.0], [48.0, -6.0]),
        b.arc(lower, [50.0, -6.0], [56.0, 0.0]),
    ];
    let results = vec![
        b.arc(upper, [59.0, 0.0], [50.0, 9.0]),
        b.line([50.0, 9.0], [48.0, 9.0]),
        b.arc(left_top, [48.0, 9.0], [40.0, 1.0]),
        b.line([40.0, 1.0], [40.0, -1.0]),
        b.arc(left_bottom, [40.0, -1.0], [48.0, -9.0]),
        b.line([48.0, -9.0], [50.0, -9.0]),
        b.arc(lower, [50.0, -9.0], [59.0, 0.0]),
    ];
    // The helpers: 3 mm on a radius of each arc, from the source to the
    // result.
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let helpers = vec![
        b.line([50.0 + 6.0 * h, 6.0 * h], [50.0 + 9.0 * h, 9.0 * h]),
        b.line(
            [48.0 - 5.0 * h, 1.0 + 5.0 * h],
            [48.0 - 8.0 * h, 1.0 + 8.0 * h],
        ),
        b.line(
            [48.0 - 5.0 * h, -1.0 - 5.0 * h],
            [48.0 - 8.0 * h, -1.0 - 8.0 * h],
        ),
        b.line([50.0 + 6.0 * h, -6.0 * h], [50.0 + 9.0 * h, -9.0 * h]),
    ];
    for e in b.def.entities.iter_mut() {
        e.reference = curves.contains(&e.id);
        e.construction = helpers.contains(&e.id);
    }
    let offsets = vec![SketchOffset {
        id: ConstraintUid(1),
        curves,
        distance: ParamId::from_raw(0),
        left: false,
        results: results.clone(),
        corners: Vec::new(),
        helpers,
        derived: false,
    }];
    let point = |entities: &[Entity], id: EntityUid| match entities
        .iter()
        .find(|e| e.id == id)
        .map(|e| &e.kind)
    {
        Some(EntityKind::Point { at }) => *at,
        _ => panic!("no point {id:?}"),
    };
    let start = |entities: &[Entity], id: EntityUid| match entities
        .iter()
        .find(|e| e.id == id)
        .map(|e| &e.kind)
    {
        Some(EntityKind::Line { start, .. } | EntityKind::Arc { start, .. }) => {
            point(entities, *start)
        }
        _ => panic!("no curve {id:?}"),
    };
    let mut entities = b.def.entities.clone();
    seed(&offsets, &mut entities, &mut |_| Ok(3.0));
    // Each corner where the offset curves meet: the results' starts in
    // chain order, the first the two arcs' corner on the split circle.
    let corners = [
        [59.0, 0.0],
        [50.0, 9.0],
        [48.0, 9.0],
        [40.0, 1.0],
        [40.0, -1.0],
        [48.0, -9.0],
        [50.0, -9.0],
    ];
    for (result, expected) in results.iter().zip(corners) {
        let at = start(&entities, *result);
        assert!(
            (at[0] - expected[0]).abs() < 1e-6 && (at[1] - expected[1]).abs() < 1e-6,
            "{result:?}: {at:?}, expected {expected:?}"
        );
    }
    // Drawn in place, the offset stays where it is.
    for (e, f) in b.def.entities.iter().zip(&entities) {
        if let (EntityKind::Point { at: p }, EntityKind::Point { at: q }) = (&e.kind, &f.kind) {
            assert!(
                (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6,
                "{:?}: {q:?}, drawn at {p:?}",
                e.id
            );
        }
    }
}

/// The corners of an offset made by constraints that the solve holds where
/// the results touch (mitcad#890): where the sources touch by their
/// definition (fixed curves that touch at their shared point, curves
/// tangent by a constraint, results of an offset whose corner is held so),
/// between a line and an arc or two arcs; not a corner of fixed curves that
/// meet at an angle, nor one of free curves that only happen to touch.
#[test]
fn tangent_corners_of_offsets_are_those_whose_sources_touch_by_definition() {
    use super::offset::{SketchOffset, tangent_corners};
    use super::pattern::{Binding, BindingKind};
    let r = 12.2;
    let offset_of = |id: u32, curves: Vec<EntityUid>, results: Vec<EntityUid>| SketchOffset {
        id: ConstraintUid(id),
        curves,
        distance: ParamId::from_raw(0),
        left: false,
        results,
        corners: Vec::new(),
        helpers: Vec::new(),
        derived: false,
    };
    let held = |b: &[Binding]| -> Vec<(u32, u32, u32, u32)> {
        b.iter()
            .map(|b| match b.kind {
                BindingKind::TangentAt { a, b: o, at } => (b.owner.0, a.0, o.0, at.0),
                other => panic!("{other:?}"),
            })
            .collect()
    };
    // A line going on tangentially into an arc (fixed, as projected), its
    // offset in by 2, and that in by 1 again.
    let mut b = Builder::new();
    let line = b.line([60.0, 0.0], [0.0, 0.0]); // c1
    let arc = b.arc([0.0, r], [-r, r], [0.0, 0.0]); // c2
    let first_line = b.line([60.0, 2.0], [0.0, 2.0]); // c3
    let first_arc = b.arc([0.0, r], [2.0 - r, r], [0.0, 2.0]); // c4
    let second_line = b.line([60.0, 3.0], [0.0, 3.0]); // c5
    let second_arc = b.arc([0.0, r], [3.0 - r, r], [0.0, 3.0]); // c6
    for e in b.def.entities.iter_mut() {
        e.fixed = e.id == line || e.id == arc;
    }
    let offsets = vec![
        offset_of(1, vec![line, arc], vec![first_line, first_arc]),
        offset_of(
            2,
            vec![first_line, first_arc],
            vec![second_line, second_arc],
        ),
    ];
    let corner = |b: &Builder, x: f64, y: f64| {
        b.def
            .entities
            .iter()
            .find(|e| matches!(e.kind, EntityKind::Point { at } if at == [x, y]))
            .unwrap()
            .id
            .0
    };
    let (c1, c2) = (corner(&b, 0.0, 2.0), corner(&b, 0.0, 3.0));
    assert_eq!(
        held(&tangent_corners(&offsets, &b.def.entities, &[])),
        [(1, 3, 4, c1), (2, 5, 6, c2)]
    );
    // Free source curves that only touch: nothing held, also not the
    // offset of their offset.
    let mut free = b.def.entities.clone();
    for e in &mut free {
        e.fixed = false;
    }
    assert!(tangent_corners(&offsets, &free, &[]).is_empty());
    // Tangent by a constraint: held.
    let tangent = [Constraint::new(
        ConstraintUid(9),
        ConstraintKind::Tangent {
            a: line,
            b: arc,
            at: None,
        },
    )];
    assert_eq!(held(&tangent_corners(&offsets, &free, &tangent)).len(), 2);
    // Fixed curves that meet at an angle (the arc a degree turned from the
    // line where they meet): nothing held.
    let mut b = Builder::new();
    let (s, c) = (1f64.to_radians().sin(), 1f64.to_radians().cos());
    let line = b.line([60.0, 0.0], [0.0, 0.0]);
    let arc = b.arc([r * s, r * c], [r * s - r, r * c], [0.0, 0.0]);
    let result_line = b.line([60.0, 2.0], [0.0, 2.0]);
    let result_arc = b.arc([r * s, r * c], [r * s - r + 2.0, r * c], [0.0, 2.0]);
    for e in b.def.entities.iter_mut() {
        e.fixed = e.id == line || e.id == arc;
    }
    let offsets = vec![offset_of(1, vec![line, arc], vec![result_line, result_arc])];
    assert!(tangent_corners(&offsets, &b.def.entities, &[]).is_empty());
    // Two arcs of one circle, each with a centre of its own a rounding
    // apart (a projected circle, mitcad#709), and a third arc touching the
    // second from outside: only the corner of the two circles that touch is
    // held.
    let mut b = Builder::new();
    let (upper, lower) = ([0.0, 9e-16], [0.0, -7e-16]);
    let left = b.arc(upper, [10.0, 0.0], [-10.0, 0.0]);
    let right = b.arc(lower, [-10.0, 0.0], [10.0, 0.0]);
    let beyond = b.arc([15.0, 0.0], [15.0, -5.0], [10.0, 0.0]);
    let results = [
        b.arc(upper, [12.0, 0.0], [-12.0, 0.0]),
        b.arc(lower, [-12.0, 0.0], [12.0, 0.0]),
        b.arc([15.0, 0.0], [15.0, -3.0], [12.0, 0.0]),
    ];
    for e in b.def.entities.iter_mut() {
        e.fixed = [left, right, beyond].contains(&e.id);
    }
    let offsets = vec![offset_of(1, vec![left, right, beyond], results.to_vec())];
    let at = corner(&b, 12.0, 0.0);
    assert_eq!(
        held(&tangent_corners(&offsets, &b.def.entities, &[])),
        [(1, results[1].0, results[2].0, at)]
    );
}

#[test]
fn plane_crossings_of_lines_conics_splines_and_points() {
    // mitcad#998: where the plane y = 0 crosses model curves.
    use crate::kernel::Curve3;
    use crate::project::plane_crossings;
    let (origin, normal) = ([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9);
    let same = |found: Vec<[f64; 3]>, want: &[[f64; 3]]| {
        found.len() == want.len() && found.iter().zip(want).all(|(a, b)| near(*a, *b))
    };
    // A line through the plane, one touching it at its start, one lying
    // in it (its ends) and one beside it.
    let line = |start, end| Curve3::Line { start, end };
    let found = plane_crossings(&line([1.0, -2.0, 0.0], [1.0, 2.0, 4.0]), origin, normal);
    assert!(same(found.clone(), &[[1.0, 0.0, 2.0]]), "{found:?}");
    let found = plane_crossings(&line([3.0, 0.0, 0.0], [3.0, 5.0, 0.0]), origin, normal);
    assert!(same(found.clone(), &[[3.0, 0.0, 0.0]]), "{found:?}");
    let found = plane_crossings(&line([0.0, 0.0, 0.0], [5.0, 0.0, 1.0]), origin, normal);
    assert!(
        same(found.clone(), &[[0.0, 0.0, 0.0], [5.0, 0.0, 1.0]]),
        "{found:?}"
    );
    assert!(plane_crossings(&line([0.0, 1.0, 0.0], [5.0, 2.0, 0.0]), origin, normal).is_empty());
    // A circle of radius 10 on XY round (0, 0): at x = 10 and x = −10, in
    // its order from its start; the quarter arc from 0 to 90° only at its
    // start; a circle on XZ lies in the plane, closed: no ends.
    let conic = |normal, start: f64, end: f64, closed| Curve3::Conic {
        center: [0.0, 0.0, 0.0],
        normal,
        x_axis: [1.0, 0.0, 0.0],
        major: 10.0,
        minor: 10.0,
        start,
        end,
        closed,
    };
    let circle = conic([0.0, 0.0, 1.0], 0.0, 2.0 * PI, true);
    let found = plane_crossings(&circle, origin, normal);
    assert!(
        same(found.clone(), &[[10.0, 0.0, 0.0], [-10.0, 0.0, 0.0]]),
        "{found:?}"
    );
    let quarter = conic([0.0, 0.0, 1.0], 0.0, PI / 2.0, false);
    let found = plane_crossings(&quarter, origin, normal);
    assert!(same(found.clone(), &[[10.0, 0.0, 0.0]]), "{found:?}");
    let upper = conic([0.0, 0.0, 1.0], PI / 4.0, 3.0 * PI / 4.0, false);
    assert!(plane_crossings(&upper, origin, normal).is_empty());
    let lying = conic([0.0, 1.0, 0.0], 0.0, 2.0 * PI, true);
    assert!(plane_crossings(&lying, origin, normal).is_empty());
    // A cubic spline from y = −1 to y = 3 crosses once; a point on the
    // plane is itself.
    let spline = Curve3::BSpline {
        degree: 3,
        poles: vec![
            [0.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [2.0, 3.0, 0.0],
            [3.0, 3.0, 0.0],
        ],
        weights: Vec::new(),
        knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
    };
    let found = plane_crossings(&spline, origin, normal);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0][1].abs() < 1e-9, "{found:?}");
    let found = plane_crossings(&Curve3::Point([4.0, 0.0, 1.0]), origin, normal);
    assert!(same(found, &[[4.0, 0.0, 1.0]]));
}
