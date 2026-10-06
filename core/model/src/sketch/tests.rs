// SPDX-License-Identifier: MIT
//! Sketches: definitions, solving, regions and keys.

use std::f64::consts::PI;

use super::geometry::{Nurbs, clamped_uniform_knots};
use super::regions::{Region, RegionCurve, regions};
use super::solve::{DimensionInput, SolverItem, solve};
use super::*;
use crate::features::sketch::SketchDef;
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
        self.curve(EntityKind::Circle { center, radius })
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
    use crate::features::sketch::SketchOutput;
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
        regions: info.iter().map(|r| r.profile.clone()).collect(),
        region_info: info,
        solved: std::sync::Arc::new(solved),
        texts: Default::default(),
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
    let horizontal = Constraint {
        id: ConstraintUid(1),
        kind: ConstraintKind::Horizontal { line: c(3) },
    };
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
    let vertical = Constraint {
        id: ConstraintUid(3),
        kind: ConstraintKind::Vertical { line: c(3) },
    };
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
