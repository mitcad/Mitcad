// SPDX-License-Identifier: MIT
//! The cut sheet's outlines, placement, overlaps and files (mitcad#853) on
//! projections made by hand: what the kernel's hidden-line removal gives
//! for a plate seen along its normal (its top and bottom edges on each
//! other, every hole's two rims, a seam across its face that ends on the
//! outline), with the body's material told by the plate's own shape.

use super::*;
use crate::{ViewEdge, ViewEdgeKind};

const EPS: f64 = 1e-6;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPS
}

fn line(a: P2, b: P2) -> ViewEdge {
    ViewEdge {
        visible: true,
        kind: ViewEdgeKind::Sharp,
        curve: ViewCurve::Line(a, b),
        source: None,
    }
}

fn arc(center: P2, radius: f64, start: f64, end: f64) -> ViewEdge {
    ViewEdge {
        visible: true,
        kind: ViewEdgeKind::Sharp,
        curve: ViewCurve::Arc {
            center,
            radius,
            start,
            end,
        },
        source: None,
    }
}

/// A 100 x 60 plate with a hole of 10 at (20, 30), one of 8 at (80, 45)
/// and a slot 8 wide from (50, 15) to (70, 15), as the projection along
/// its normal sees it: every edge twice (top and bottom), and a seam
/// across its face at x = 30 that ends on the outline.
fn plate_edges() -> Vec<ViewEdge> {
    let mut edges = Vec::new();
    for _ in 0..2 {
        edges.push(line([0.0, 0.0], [100.0, 0.0]));
        edges.push(line([100.0, 0.0], [100.0, 60.0]));
        edges.push(line([100.0, 60.0], [0.0, 60.0]));
        edges.push(line([0.0, 60.0], [0.0, 0.0]));
        edges.push(arc([20.0, 30.0], 5.0, 0.0, TAU));
        edges.push(arc([80.0, 45.0], 4.0, 0.0, TAU));
        edges.push(line([50.0, 11.0], [70.0, 11.0]));
        edges.push(line([70.0, 19.0], [50.0, 19.0]));
        edges.push(arc([70.0, 15.0], 4.0, -PI / 2.0, PI / 2.0));
        edges.push(arc([50.0, 15.0], 4.0, PI / 2.0, 3.0 * PI / 2.0));
    }
    edges.push(line([30.0, 0.0], [30.0, 60.0]));
    // A vertical edge seen end on: nothing.
    edges.push(line([0.0, 0.0], [0.0, 0.0]));
    edges
}

fn in_slot(p: P2) -> bool {
    let x = p[0].clamp(50.0, 70.0);
    dist(p, [x, 15.0]) < 4.0
}

fn plate_material(p: P2) -> bool {
    p[0] > 0.0
        && p[0] < 100.0
        && p[1] > 0.0
        && p[1] < 60.0
        && dist(p, [20.0, 30.0]) > 5.0
        && dist(p, [80.0, 45.0]) > 4.0
        && !in_slot(p)
}

fn classify(points: &[P2]) -> Result<Vec<bool>, String> {
    Ok(points.iter().map(|p| plate_material(*p)).collect())
}

fn plate() -> Outline {
    silhouette(&plate_edges(), classify).unwrap()
}

#[test]
fn a_plate_with_two_holes_and_a_slot_is_one_outer_loop_and_three_inner() {
    let outline = plate();
    assert_eq!(outline.outer(), 1, "{outline:?}");
    assert_eq!(outline.inner(), 3, "{outline:?}");
    // The outer boundary: the four sides, each one line again across the
    // seam that split two of them, counter-clockwise.
    let outer = &outline.loops[0];
    assert!(outer.outer);
    assert_eq!(outer.pieces.len(), 4, "{:?}", outer.pieces);
    assert!(near(outer.signed_area(), 6000.0), "{}", outer.signed_area());
    let (lo, hi) = bounds(std::slice::from_ref(outer)).unwrap();
    assert!(near(lo[0], 0.0) && near(lo[1], 0.0) && near(hi[0], 100.0) && near(hi[1], 60.0));
    for piece in &outer.pieces {
        let Piece::Line(a, b) = piece else {
            panic!("not a line: {piece:?}");
        };
        let l = dist(*a, *b);
        assert!(near(l, 100.0) || near(l, 60.0), "{l}");
    }
    // The holes: whole circles, clockwise (material on their left).
    let circles: Vec<(P2, f64)> = outline.loops[1..]
        .iter()
        .filter_map(|l| match l.pieces.as_slice() {
            [
                Piece::Arc {
                    center,
                    radius,
                    sweep,
                    ..
                },
            ] => {
                assert!(near(*sweep, -TAU), "{sweep}");
                Some((*center, *radius))
            }
            _ => None,
        })
        .collect();
    assert_eq!(circles.len(), 2, "{:?}", outline.loops);
    assert!(
        circles
            .iter()
            .any(|(c, r)| near(c[0], 20.0) && near(c[1], 30.0) && near(*r, 5.0))
    );
    assert!(
        circles
            .iter()
            .any(|(c, r)| near(c[0], 80.0) && near(c[1], 45.0) && near(*r, 4.0))
    );
    // The slot: two lines of 20 and two half circles of 4.
    let slot = outline.loops[1..]
        .iter()
        .find(|l| l.pieces.len() == 4)
        .expect("the slot");
    assert!(!slot.outer);
    let lines: Vec<f64> = slot
        .pieces
        .iter()
        .filter(|p| matches!(p, Piece::Line(..)))
        .map(|p| p.length())
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|l| near(*l, 20.0)), "{lines:?}");
    for p in &slot.pieces {
        if let Piece::Arc { radius, sweep, .. } = p {
            assert!(near(*radius, 4.0) && near(sweep.abs(), PI), "{p:?}");
        }
    }
    let area = 6000.0 - 25.0 * PI - 16.0 * PI - (160.0 + 16.0 * PI);
    let total: f64 = outline.loops.iter().map(OutlineLoop::signed_area).sum();
    assert!((total - area).abs() < 0.05, "{total} {area}");
}

#[test]
fn a_blind_hole_is_no_cut() {
    // The material behind the smaller hole: a blind hole seen from its
    // open side, whose rims the projection still shows.
    let outline = silhouette(&plate_edges(), |points: &[P2]| {
        Ok(points
            .iter()
            .map(|p| plate_material(*p) || dist(*p, [80.0, 45.0]) < 4.0)
            .collect())
    })
    .unwrap();
    assert_eq!((outline.outer(), outline.inner()), (1, 2));
    // Nothing seen: no loops.
    let none = silhouette(&[], classify).unwrap();
    assert!(none.loops.is_empty());
    // A failed classification fails the outline.
    assert!(silhouette(&plate_edges(), |_: &[P2]| Err("no".to_owned())).is_err());
}

#[test]
fn a_part_is_placed_by_its_middle_and_turned_about_it() {
    let outline = plate();
    let loops = placed(&outline, [300.0, 150.0], 90.0);
    let (lo, hi) = bounds(&loops).unwrap();
    // 60 wide and 100 high, its middle at the place.
    assert!(
        near(hi[0] - lo[0], 60.0) && near(hi[1] - lo[1], 100.0),
        "{lo:?} {hi:?}"
    );
    assert!(near((lo[0] + hi[0]) / 2.0, 300.0) && near((lo[1] + hi[1]) / 2.0, 150.0));
    // The hole of 10 at (20, 30) of the plate, 30 left and 0 below its
    // middle, turned a quarter: 0 right and 30 below.
    assert!(loops.iter().any(|l| matches!(l.pieces.as_slice(),
        [Piece::Arc { center, radius, .. }]
            if near(*radius, 5.0) && near(center[0], 300.0) && near(center[1], 120.0))));
    assert!(!off_sheet(&loops, 600.0, 300.0));
    assert!(off_sheet(
        &placed(&outline, [590.0, 150.0], 0.0),
        600.0,
        300.0
    ));
}

#[test]
fn overlaps_are_told_from_touching_and_nesting() {
    let outline = plate();
    let a = placed(&outline, [100.0, 100.0], 0.0);
    // Overlapping by 10 mm.
    assert!(overlap(&a, &placed(&outline, [190.0, 100.0], 0.0)));
    // Side by side, touching.
    assert!(!overlap(&a, &placed(&outline, [200.0, 100.0], 0.0)));
    // Apart.
    assert!(!overlap(&a, &placed(&outline, [100.0, 200.0], 0.0)));
    // A small part in the plate's hole of 10: nested, not overlapping; on
    // the plate's material: overlapping.
    let small = Outline {
        loops: vec![OutlineLoop {
            outer: true,
            pieces: vec![Piece::Arc {
                center: [0.0, 0.0],
                radius: 2.0,
                start: 0.0,
                sweep: TAU,
            }],
        }],
    };
    assert!(!overlap(&a, &placed(&small, [70.0, 100.0], 0.0)));
    assert!(overlap(&a, &placed(&small, [120.0, 110.0], 0.0)));
}

fn parts() -> Vec<PlacedPart> {
    let outline = plate();
    vec![
        PlacedPart {
            id: 1,
            name: "Rib 3".to_owned(),
            loops: placed(&outline, [60.0, 40.0], 0.0),
        },
        PlacedPart {
            id: 2,
            name: "Rib 3".to_owned(),
            loops: placed(&outline, [180.0, 40.0], 0.0),
        },
    ]
}

#[test]
fn the_dxf_file_reads_back_with_its_layers_and_sizes() {
    let drawing = dxf(&parts(), true);
    let text = mitcad_dxf::write_string(&drawing, &mitcad_dxf::WriteOptions::default()).unwrap();
    assert!(text.contains("AC1015"), "R2000");
    let back = mitcad_dxf::read_str(&text).unwrap();
    assert_eq!(back.units, mitcad_dxf::Units::Millimeters);
    for layer in [CUT_OUTER, CUT_INNER, LABEL] {
        assert!(back.layer(layer).is_some(), "{layer}");
    }
    let on = |layer: &str| -> Vec<&mitcad_dxf::Geometry> {
        back.entities
            .iter()
            .filter(|e| e.layer == layer)
            .map(|e| &e.geometry)
            .collect()
    };
    // Two parts of four sides each; two circles, two slot lines and two
    // half circles each; a name each.
    assert_eq!(on(CUT_OUTER).len(), 8);
    let inner = on(CUT_INNER);
    assert_eq!(inner.len(), 12);
    let circles = inner
        .iter()
        .filter(|g| matches!(g, mitcad_dxf::Geometry::Circle { .. }))
        .count();
    let arcs = inner
        .iter()
        .filter(|g| matches!(g, mitcad_dxf::Geometry::Arc { radius, .. } if near(*radius, 4.0)))
        .count();
    assert_eq!((circles, arcs), (4, 4));
    assert_eq!(on(LABEL).len(), 2);
    // At 1:1: the outer boundaries span 10..230 x 10..70.
    let (min, max) = back.on_layers(&[CUT_OUTER.to_owned()]).bounds().unwrap();
    assert!(near(min.x, 10.0) && near(min.y, 10.0) && near(max.x, 230.0) && near(max.y, 70.0));
    // Without labels, no LABEL layer.
    let plain = dxf(&parts(), false);
    assert!(plain.layer(LABEL).is_none());
    assert!(plain.entities.iter().all(|e| e.layer != LABEL));
}

#[test]
fn a_polyline_is_a_spline_of_degree_one_and_a_path() {
    let part = PlacedPart {
        id: 1,
        name: "Curve".to_owned(),
        loops: vec![OutlineLoop {
            outer: true,
            pieces: vec![
                Piece::Polyline(vec![[0.0, 0.0], [10.0, 1.0], [20.0, 0.0]]),
                Piece::Line([20.0, 0.0], [0.0, 0.0]),
            ],
        }],
    };
    let drawing = dxf(std::slice::from_ref(&part), false);
    let text = mitcad_dxf::write_string(&drawing, &mitcad_dxf::WriteOptions::default()).unwrap();
    let back = mitcad_dxf::read_str(&text).unwrap();
    let spline = back
        .entities
        .iter()
        .find_map(|e| match &e.geometry {
            mitcad_dxf::Geometry::Spline(s) => Some(s.clone()),
            _ => None,
        })
        .expect("a spline");
    assert_eq!(spline.degree, 1);
    assert_eq!(spline.control_points.len(), 3);
    let mid = spline.point_at(1.0).unwrap();
    assert!(near(mid.x, 10.0) && near(mid.y, 1.0), "{mid:?}");
    let svg = svg(&[part], 100.0, 50.0, false);
    assert!(
        svg.contains(r#"d="M 0 50 L 10 49 L 20 50 L 0 50 Z""#),
        "{svg}"
    );
}

#[test]
fn the_svg_file_has_its_layers_at_the_material_size() {
    let text = svg(&parts(), 600.0, 300.0, true);
    assert!(text.contains(r#"width="600mm" height="300mm" viewBox="0 0 600 300""#));
    let layer = |name: &str| {
        let start = text.find(&format!(r#"<g id="{name}""#)).expect(name);
        let end = start + text[start..].find("</g>").unwrap();
        text[start..end].to_owned()
    };
    // A closed path per loop: two outer, six inner.
    assert_eq!(layer(CUT_OUTER).matches("<path").count(), 2);
    assert_eq!(layer(CUT_INNER).matches("<path").count(), 6);
    assert!(layer(CUT_OUTER).matches(" Z\"").count() == 2);
    // The first part's outline from (10, 10) to (110, 70) of the sheet:
    // y down from the top in SVG.
    assert!(layer(CUT_OUTER).contains("M 10 290") || layer(CUT_OUTER).contains("290"));
    // Circles as two arcs of their radius.
    assert!(layer(CUT_INNER).contains("A 5 5 0 0"));
    assert_eq!(layer(LABEL).matches(">Rib 3</text>").count(), 2);
    assert!(!svg(&parts(), 600.0, 300.0, false).contains(LABEL));
}

#[test]
fn the_sheet_drawn_has_a_group_per_part() {
    let drawn = sheet(600.0, 300.0, "Cut", &parts());
    let views: Vec<_> = drawn.groups_of("view").collect();
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].id.as_deref(), Some("part-1"));
    // Four sides, two circles, the slot's four pieces.
    assert_eq!(views[0].items.len(), 10);
    assert_eq!(drawn.groups_of("view-label").count(), 2);
}

// --- B-splines (mitcad#853) --------------------------------------------------

use crate::ViewSpline;
use crate::bspline::{Spline2, cubic};

/// A cubic B-spline of one span from (100, 60) to (0, 60), arching up to
/// about 75 and dipping to about 52: the top of a plate.
fn top_curve() -> ViewSpline {
    let poles = vec![[100.0, 60.0], [70.0, 80.0], [30.0, 40.0], [0.0, 60.0]];
    let spline = Spline2 {
        degree: 3,
        knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        poles: poles.clone(),
        weights: Vec::new(),
    };
    let (a, b) = spline.domain();
    ViewSpline {
        degree: 3,
        knots: spline.knots.clone(),
        poles,
        weights: Vec::new(),
        points: spline.samples(a, b, 0.01).iter().map(|(_, p)| *p).collect(),
    }
}

/// A plate of three lines and that curve on top, seen with every edge
/// twice (top and bottom), and a seam from (50, 0) up to the curve's
/// middle that splits it.
fn curved_plate() -> (Vec<ViewEdge>, Spline2) {
    let curve = top_curve();
    let spline = Spline2::of(&curve).unwrap();
    let mid = spline.point(0.5);
    let mut edges = Vec::new();
    for _ in 0..2 {
        edges.push(line([0.0, 0.0], [100.0, 0.0]));
        edges.push(line([100.0, 0.0], [100.0, 60.0]));
        edges.push(ViewEdge {
            visible: true,
            kind: ViewEdgeKind::Sharp,
            curve: ViewCurve::Spline(curve.clone()),
            source: None,
        });
        edges.push(line([0.0, 60.0], [0.0, 0.0]));
    }
    edges.push(line([50.0, 0.0], mid));
    (edges, spline)
}

/// Whether a point lies in the curved plate, against its outline sampled
/// within a nanometre.
fn in_curved_plate(spline: &Spline2, p: P2) -> bool {
    let mut polygon = vec![[0.0, 0.0], [100.0, 0.0]];
    polygon.extend(spline.samples(0.0, 1.0, 1e-9).iter().map(|(_, q)| *q));
    in_material(&[polygon], p)
}

fn curved_outline() -> (Outline, Spline2) {
    let (edges, spline) = curved_plate();
    let outline = silhouette(&edges, |points: &[P2]| {
        Ok(points
            .iter()
            .map(|p| in_curved_plate(&spline, *p))
            .collect())
    })
    .unwrap();
    (outline, spline)
}

fn same_points(a: &[P2], b: &[P2]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| near(p[0], q[0]) && near(p[1], q[1]))
}

#[test]
fn a_spline_is_cut_and_turned_exactly() {
    let spline = Spline2::of(&top_curve()).unwrap();
    // A part of it is the same curve there.
    let part = spline.segment(0.2, 0.7);
    let (a, b) = part.domain();
    assert!(near(a, 0.2) && near(b, 0.7));
    for i in 0..=10 {
        let u = 0.2 + 0.05 * f64::from(i);
        assert!(dist(part.point(u), spline.point(u)) < 1e-9, "{u}");
    }
    // The other way round.
    let back = spline.reversed();
    assert!(dist(back.point(0.3), spline.point(0.7)) < 1e-9);
    // A rational quarter circle stays on its circle when cut.
    let w = std::f64::consts::FRAC_1_SQRT_2;
    let arc = Spline2 {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        poles: vec![[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        weights: vec![1.0, w, 1.0],
    };
    assert!(arc.is_rational());
    let piece = arc.segment(0.25, 0.9);
    for i in 0..=10 {
        let u = 0.25 + 0.065 * f64::from(i);
        assert!((norm(piece.point(u)) - 1.0).abs() < 1e-12, "{u}");
        assert!(dist(piece.point(u), arc.point(u)) < 1e-12);
    }
    // Two spans as two Bézier pieces; a quadratic raised to a cubic.
    let two = Spline2 {
        degree: 3,
        knots: vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
        poles: vec![[0.0, 0.0], [1.0, 2.0], [2.0, -1.0], [3.0, 2.0], [4.0, 0.0]],
        weights: Vec::new(),
    };
    let pieces = two.beziers().unwrap();
    assert_eq!(pieces.len(), 2);
    assert!(dist(pieces[0][3], two.point(0.5)) < 1e-12);
    let c = cubic(&[[0.0, 0.0], [1.0, 2.0], [2.0, 0.0]]);
    assert!(
        dist(c[1], [2.0 / 3.0, 4.0 / 3.0]) < 1e-12 && dist(c[2], [4.0 / 3.0, 4.0 / 3.0]) < 1e-12
    );
    assert!(arc.beziers().is_none());
}

#[test]
fn a_plate_with_a_spline_edge_keeps_it_as_a_spline() {
    let (outline, spline) = curved_outline();
    assert_eq!((outline.outer(), outline.inner()), (1, 0), "{outline:?}");
    let outer = &outline.loops[0];
    // Three lines and the curve, split by the seam and joined again: the
    // curve as it was given, the way the outline runs (counter-clockwise:
    // right to left along the top).
    assert_eq!(outer.pieces.len(), 4, "{:?}", outer.pieces);
    let curves: Vec<Spline2> = outer.pieces.iter().filter_map(Piece::exact).collect();
    assert_eq!(curves.len(), 1);
    assert_eq!(curves[0].degree, 3);
    assert!(
        same_points(&curves[0].poles, &spline.poles),
        "{:?}",
        curves[0]
    );
    assert_eq!(curves[0].knots, spline.knots);
    // The area under the curve.
    let area = outer.signed_area();
    let polygon_area = signed_area(&{
        let mut p = vec![[0.0, 0.0], [100.0, 0.0]];
        p.extend(spline.samples(0.0, 1.0, 1e-9).iter().map(|(_, q)| *q));
        p
    });
    assert!((area - polygon_area).abs() < 0.05, "{area} {polygon_area}");
}

#[test]
fn a_spline_edge_is_written_as_a_dxf_spline_and_an_svg_path() {
    let (outline, spline) = curved_outline();
    // Placed with its box's middle where it is: the sheet's coordinates
    // are the plate's.
    let (lo, hi) = outline.bounds().unwrap();
    let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let part = PlacedPart {
        id: 1,
        name: "Curved".to_owned(),
        loops: placed(&outline, mid, 0.0),
    };
    let text = mitcad_dxf::write_string(
        &dxf(std::slice::from_ref(&part), false),
        &mitcad_dxf::WriteOptions::default(),
    )
    .unwrap();
    let back = mitcad_dxf::read_str(&text).unwrap();
    let splines: Vec<_> = back
        .entities
        .iter()
        .filter_map(|e| match &e.geometry {
            mitcad_dxf::Geometry::Spline(s) => Some((e.layer.clone(), s.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(splines.len(), 1, "{text}");
    let (layer, read) = &splines[0];
    assert_eq!(layer, CUT_OUTER);
    assert_eq!(read.degree, 3);
    assert!(read.weights.is_empty() || read.weights.iter().all(|w| near(*w, 1.0)));
    let poles: Vec<P2> = read.control_points.iter().map(|q| [q.x, q.y]).collect();
    assert!(same_points(&poles, &spline.poles), "{poles:?}");
    assert_eq!(read.knots, spline.knots);
    assert_eq!(
        back.entities
            .iter()
            .filter(|e| matches!(e.geometry, mitcad_dxf::Geometry::Line { .. }))
            .count(),
        3
    );
    // The SVG: the curve as one cubic Bézier of its own poles (y down from
    // the top of a sheet 80 high).
    let svg = svg(&[part], 120.0, 80.0, false);
    assert!(svg.contains(" C 70 0 30 40 0 20"), "{svg}");
    assert!(svg.contains("M ") && svg.contains(" Z\""));
}

#[test]
fn a_spline_turned_on_the_sheet_keeps_its_degree_and_moves_its_poles() {
    let (outline, spline) = curved_outline();
    let loops = placed(&outline, [300.0, 150.0], 90.0);
    let mid = middle(&outline);
    let turned: Vec<Spline2> = loops
        .iter()
        .flat_map(|l| &l.pieces)
        .filter_map(Piece::exact)
        .collect();
    assert_eq!(turned.len(), 1);
    // Each pole turned a quarter about the middle.
    for (p, q) in turned[0].poles.iter().zip(&spline.poles) {
        let d = [q[0] - mid[0], q[1] - mid[1]];
        let expected = [300.0 - d[1], 150.0 + d[0]];
        assert!(
            near(p[0], expected[0]) && near(p[1], expected[1]),
            "{p:?} {expected:?}"
        );
    }
}
