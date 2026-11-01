// SPDX-License-Identifier: MIT
//! The line set, details smaller than the pen and threads by ISO 6410
//! (mitcad#1112), on views as the kernel would project them.

use std::f64::consts::{FRAC_PI_2, TAU};

use super::*;
use crate::sheet::{Group, Sheet};

fn line(a: P2, b: P2) -> ViewEdge {
    ViewEdge {
        visible: true,
        kind: ViewEdgeKind::Sharp,
        curve: ViewCurve::Line(a, b),
        source: None,
    }
}

fn kind_line(kind: ViewEdgeKind, visible: bool, a: P2, b: P2) -> ViewEdge {
    ViewEdge {
        visible,
        kind,
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

fn polyline(kind: ViewEdgeKind, points: Vec<P2>) -> ViewEdge {
    ViewEdge {
        visible: true,
        kind,
        curve: ViewCurve::Polyline(points),
        source: None,
    }
}

fn near(a: P2, b: P2) -> bool {
    vec2::distance(a, b) < 1e-6
}

fn same(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn lines(edges: &[ViewEdge]) -> Vec<(P2, P2)> {
    edges
        .iter()
        .filter_map(|e| match e.curve {
            ViewCurve::Line(a, b) => Some((a, b)),
            _ => None,
        })
        .collect()
}

/// Whether a line joins the two points (either way round).
fn has_line(edges: &[ViewEdge], p: P2, q: P2) -> bool {
    lines(edges)
        .iter()
        .any(|(a, b)| (near(*a, p) && near(*b, q)) || (near(*a, q) && near(*b, p)))
}

/// A 20 x 20 square whose upper right corner is rounded by `corner`,
/// which runs from (20, 20 - r) to (20 - r, 20).
fn square_with(r: f64, corner: Vec<ViewEdge>) -> Vec<ViewEdge> {
    let mut edges = vec![
        line([0.0, 0.0], [20.0, 0.0]),
        line([20.0, 0.0], [20.0, 20.0 - r]),
    ];
    edges.extend(corner);
    edges.push(line([20.0 - r, 20.0], [0.0, 20.0]));
    edges.push(line([0.0, 20.0], [0.0, 0.0]));
    edges
}

fn front() -> ViewSpec {
    ViewSpec {
        toward_eye: [0.0, -1.0, 0.0],
        x_axis: [1.0, 0.0, 0.0],
        hidden: true,
        smooth: true,
        exact: true,
        quick: false,
        section: None,
        deflection: 0.01,
    }
}

fn right() -> ViewSpec {
    ViewSpec {
        toward_eye: [1.0, 0.0, 0.0],
        x_axis: [0.0, 1.0, 0.0],
        ..front()
    }
}

#[test]
fn the_default_line_set_is_iso_128s_lighter_one() {
    assert!(same(Pen::Thick.width(), 0.35));
    assert!(same(Pen::Thin.width(), 0.18));
    assert!(same(Pen::Chain.width(), 0.18));
    assert!(same(Pen::Hidden.width(), 0.18));
    assert!(same(Pen::Hatch.width(), 0.13));
    assert!(same(Pen::Frame.width(), 0.5));
    assert!((detail_size() - 0.7).abs() < 1e-12);
    // The writers draw with that table.
    let mut g = Group::new("view");
    g.line(Pen::Thick, [10.0, 10.0], [50.0, 10.0]);
    g.line(Pen::Hidden, [10.0, 20.0], [50.0, 20.0]);
    let sheet = Sheet {
        width: 297.0,
        height: 210.0,
        title: "Lines".to_owned(),
        groups: vec![g],
    };
    let svg = crate::svg::write(&sheet);
    assert!(svg.contains(r#"stroke-width="0.35""#), "{svg}");
    assert!(svg.contains(r#"stroke-width="0.18""#), "{svg}");
    assert!(!svg.contains(r#"stroke-width="0.5""#), "{svg}");
    let pdf = String::from_utf8_lossy(&crate::pdf::write(&[sheet], "Lines")).into_owned();
    assert!(pdf.contains("0.35 w"), "{pdf}");
    assert!(pdf.contains("0.18 w"), "{pdf}");
}

#[test]
fn a_fillet_smaller_than_the_pen_is_a_sharp_corner() {
    // A 0.2 mm fillet at 1:1: its arc goes and the sides meet at the
    // corner.
    let r = 0.2;
    let edges = square_with(r, vec![arc([20.0 - r, 20.0 - r], r, 0.0, FRAC_PI_2)]);
    let drawn = simplified(&edges, 1.0);
    assert!(
        drawn.iter().all(|e| matches!(e.curve, ViewCurve::Line(..))),
        "{drawn:?}"
    );
    assert_eq!(drawn.len(), 4, "{drawn:?}");
    assert!(has_line(&drawn, [20.0, 0.0], [20.0, 20.0]), "{drawn:?}");
    assert!(has_line(&drawn, [20.0, 20.0], [0.0, 20.0]), "{drawn:?}");
    // At 1:2 a 0.5 mm fillet is 0.25 mm on the sheet: a corner too.
    let r = 0.5;
    let edges = square_with(r, vec![arc([20.0 - r, 20.0 - r], r, 0.0, FRAC_PI_2)]);
    let drawn = simplified(&edges, 0.5);
    assert!(has_line(&drawn, [20.0, 0.0], [20.0, 20.0]), "{drawn:?}");
    assert!(has_line(&drawn, [20.0, 20.0], [0.0, 20.0]), "{drawn:?}");
}

#[test]
fn a_fillet_from_a_triangulation_is_a_sharp_corner_too() {
    // The quick path's fillet: three short lines along the arc.
    let r = 0.2;
    let c = [20.0 - r, 20.0 - r];
    let at = |k: f64| arc_point(c, r, k * FRAC_PI_2 / 3.0);
    let corner = vec![
        line(at(0.0), at(1.0)),
        line(at(1.0), at(2.0)),
        line(at(2.0), at(3.0)),
    ];
    let drawn = simplified(&square_with(r, corner), 1.0);
    assert_eq!(drawn.len(), 4, "{drawn:?}");
    assert!(has_line(&drawn, [20.0, 0.0], [20.0, 20.0]), "{drawn:?}");
    assert!(has_line(&drawn, [20.0, 20.0], [0.0, 20.0]), "{drawn:?}");
    // And as one polyline.
    let points: Vec<P2> = (0..=6)
        .map(|k| arc_point(c, r, f64::from(k) * FRAC_PI_2 / 6.0))
        .collect();
    let drawn = simplified(
        &square_with(r, vec![polyline(ViewEdgeKind::Sharp, points)]),
        1.0,
    );
    assert_eq!(drawn.len(), 4, "{drawn:?}");
    assert!(has_line(&drawn, [20.0, 20.0], [0.0, 20.0]), "{drawn:?}");
}

#[test]
fn a_chamfer_smaller_than_the_pen_is_a_sharp_corner() {
    let c = 0.3;
    let edges = square_with(c, vec![line([20.0, 20.0 - c], [20.0 - c, 20.0])]);
    let drawn = simplified(&edges, 1.0);
    assert_eq!(drawn.len(), 4, "{drawn:?}");
    assert!(has_line(&drawn, [20.0, 0.0], [20.0, 20.0]), "{drawn:?}");
    assert!(has_line(&drawn, [20.0, 20.0], [0.0, 20.0]), "{drawn:?}");
    // A 2 mm chamfer stays.
    let c = 2.0;
    let edges = square_with(c, vec![line([20.0, 20.0 - c], [20.0 - c, 20.0])]);
    assert_eq!(simplified(&edges, 1.0), edges);
}

#[test]
fn a_large_fillet_is_still_an_arc() {
    let r = 5.0;
    let edges = square_with(r, vec![arc([20.0 - r, 20.0 - r], r, 0.0, FRAC_PI_2)]);
    let drawn = simplified(&edges, 1.0);
    assert_eq!(drawn, edges);
    // The same fillet at 1:10 is 0.5 mm on the sheet: a corner.
    let drawn = simplified(&edges, 0.1);
    assert!(
        drawn.iter().all(|e| matches!(e.curve, ViewCurve::Line(..))),
        "{drawn:?}"
    );
    assert!(has_line(&drawn, [20.0, 20.0], [0.0, 20.0]), "{drawn:?}");
}

#[test]
fn a_small_fillets_tangent_edges_go_with_it() {
    // A shaft of 8 against a shoulder at x 10, with a fillet of 0.2 in the
    // corner, seen from the side: the shaft's outline, the fillet's
    // outline (a quarter circle), the shoulder's line and the fillet's two
    // tangent edges across.
    let r = 0.2;
    let quarter = |sign: f64| -> Vec<P2> {
        (0..=8)
            .map(|k| {
                let a = -FRAC_PI_2 + f64::from(k) * FRAC_PI_2 / 8.0;
                [10.0 - r + r * a.cos() + r, sign * (4.0 + r + r * a.sin())]
            })
            .map(|p| [p[0] - r, p[1]])
            .collect()
    };
    let edges = vec![
        kind_line(ViewEdgeKind::Outline, true, [0.0, 4.0], [10.0 - r, 4.0]),
        kind_line(ViewEdgeKind::Outline, true, [0.0, -4.0], [10.0 - r, -4.0]),
        polyline(ViewEdgeKind::Outline, quarter(1.0)),
        polyline(ViewEdgeKind::Outline, quarter(-1.0)),
        line([10.0, -8.0], [10.0, 8.0]),
        kind_line(
            ViewEdgeKind::Smooth,
            true,
            [10.0 - r, -4.0],
            [10.0 - r, 4.0],
        ),
        kind_line(
            ViewEdgeKind::Smooth,
            true,
            [10.0, -4.0 - r],
            [10.0, 4.0 + r],
        ),
    ];
    let drawn = simplified(&edges, 1.0);
    assert!(
        drawn.iter().all(|e| e.kind != ViewEdgeKind::Smooth),
        "{drawn:?}"
    );
    assert!(
        drawn.iter().all(|e| matches!(e.curve, ViewCurve::Line(..))),
        "{drawn:?}"
    );
    assert!(has_line(&drawn, [0.0, 4.0], [10.0, 4.0]), "{drawn:?}");
    assert!(has_line(&drawn, [0.0, -4.0], [10.0, -4.0]), "{drawn:?}");
    assert!(has_line(&drawn, [10.0, -8.0], [10.0, 8.0]), "{drawn:?}");
    // A tangent edge 0.5 mm from an edge all along goes; one 2 mm away
    // stays.
    let edges = vec![
        line([0.0, 0.0], [30.0, 0.0]),
        kind_line(ViewEdgeKind::Smooth, true, [0.0, 0.5], [30.0, 0.5]),
        kind_line(ViewEdgeKind::Smooth, true, [0.0, 2.0], [30.0, 2.0]),
    ];
    let drawn = simplified(&edges, 1.0);
    assert_eq!(drawn, vec![edges[0].clone(), edges[2].clone()]);
}

#[test]
fn edges_shorter_than_the_pen_go() {
    let edges = vec![
        line([0.0, 0.0], [30.0, 0.0]),
        // Seen end on.
        line([5.0, 5.0], [5.0, 5.0]),
        line([10.0, 5.0], [10.2, 5.0]),
        // A hole of 0.2.
        arc([20.0, 5.0], 0.1, 0.0, TAU),
        // A hole of 1 stays.
        arc([25.0, 5.0], 0.5, 0.0, TAU),
    ];
    let drawn = simplified(&edges, 1.0);
    assert_eq!(drawn, vec![edges[0].clone(), edges[4].clone()]);
}

/// M8: major 8, minor 6.647, pitch 1.25.
const MAJOR: f64 = 8.0;
const MINOR: f64 = 6.647;
const PITCH: f64 = 1.25;

/// A shaft of 8 along X from 0 to 30 with an M8 thread from 10 to its end,
/// seen from the front: the plain part's lines, the end faces, and the
/// thread as its geometry: a helix polyline per turn at the crest and at
/// the root, and the crests' wavy outlines.
fn threaded_shaft() -> Vec<ViewEdge> {
    let mut edges = vec![
        line([0.0, 4.0], [10.0, 4.0]),
        line([0.0, -4.0], [10.0, -4.0]),
        line([0.0, -4.0], [0.0, 4.0]),
        line([30.0, -4.0], [30.0, 4.0]),
    ];
    let turns = (20.0 / PITCH) as usize;
    for k in 0..turns {
        for radius in [MAJOR / 2.0, MINOR / 2.0] {
            let points = (0..=12)
                .map(|i| {
                    let a = f64::from(i) / 12.0 * std::f64::consts::PI;
                    [10.0 + PITCH * (k as f64 + a / TAU), radius * a.cos()]
                })
                .collect();
            edges.push(polyline(ViewEdgeKind::Sharp, points));
        }
    }
    for sign in [-1.0, 1.0] {
        let points = (0..=64)
            .map(|i| {
                let x = 10.0 + 20.0 * f64::from(i) / 64.0;
                [x, sign * (3.9 + 0.1 * (x * TAU / PITCH).cos().abs())]
            })
            .collect();
        edges.push(polyline(ViewEdgeKind::Outline, points));
    }
    edges
}

#[test]
fn a_modelled_thread_is_drawn_by_iso_6410_across_its_axis() {
    let thread = ThreadInput {
        start: [10.0, 0.0, 0.0],
        end: [30.0, 0.0, 0.0],
        major: MAJOR,
        minor: MINOR,
        internal: false,
        taper: 0.0,
        source: None,
    };
    let projection = ViewProjection {
        edges: threaded_shaft(),
        ..ViewProjection::default()
    };
    let drawn = with_threads(&projection, &front(), &[thread]).edges;
    // No helix or crest edges.
    assert!(
        drawn.iter().all(|e| matches!(e.curve, ViewCurve::Line(..))),
        "{drawn:?}"
    );
    let pen_of = |a: P2, b: P2| {
        drawn
            .iter()
            .filter(|e| match e.curve {
                ViewCurve::Line(p, q) => (near(p, a) && near(q, b)) || (near(p, b) && near(q, a)),
                _ => false,
            })
            .map(|e| edge_pen(e.visible, e.kind))
            .collect::<Vec<_>>()
    };
    // The major diameter wide, the minor narrow, from 10 to 30.
    let (crest, root) = (MAJOR / 2.0, MINOR / 2.0);
    for side in [-1.0, 1.0] {
        assert_eq!(
            pen_of([10.0, side * crest], [30.0, side * crest]),
            [Pen::Thick],
            "{drawn:?}"
        );
        assert_eq!(
            pen_of([10.0, side * root], [30.0, side * root]),
            [Pen::Thin],
            "{drawn:?}"
        );
    }
    // The thread's end across the major diameter at 10; the shaft's end
    // face at 30 ends it there.
    assert_eq!(pen_of([10.0, -crest], [10.0, crest]), [Pen::Thick]);
    assert_eq!(pen_of([30.0, -crest], [30.0, crest]), [Pen::Thick]);
    // The plain part as it was.
    assert!(has_line(&drawn, [0.0, 4.0], [10.0, 4.0]));
    assert!(has_line(&drawn, [0.0, -4.0], [0.0, 4.0]));
    assert_eq!(drawn.len(), 4 + 4 + 1, "{drawn:?}");
    // On the sheet: what the thread leaves of the view needs no more
    // simplifying at 1:1.
    assert_eq!(simplified(&drawn, 1.0).len(), drawn.len());
}

#[test]
fn a_thread_seen_end_on_has_a_three_quarter_root_circle() {
    // The shaft from its threaded end: the crest and root helices as
    // circles in pieces, the end face's chamfer circle, a flange of 20.
    let mut edges = Vec::new();
    for radius in [MAJOR / 2.0, MINOR / 2.0] {
        for k in 0..4 {
            let a = f64::from(k) * FRAC_PI_2;
            edges.push(arc([0.0, 0.0], radius, a, a + FRAC_PI_2));
        }
    }
    edges.push(arc([0.0, 0.0], 3.0, 0.0, TAU));
    edges.push(arc([0.0, 0.0], 10.0, 0.0, TAU));
    let thread = ThreadInput {
        start: [30.0, 0.0, 0.0],
        end: [10.0, 0.0, 0.0],
        major: MAJOR,
        minor: MINOR,
        internal: false,
        taper: 0.0,
        source: None,
    };
    let projection = ViewProjection {
        edges,
        ..ViewProjection::default()
    };
    let drawn = with_threads(&projection, &right(), &[thread]).edges;
    let arcs: Vec<(f64, f64, Pen)> = drawn
        .iter()
        .filter_map(|e| match e.curve {
            ViewCurve::Arc {
                radius, start, end, ..
            } => Some((radius, end - start, edge_pen(e.visible, e.kind))),
            _ => None,
        })
        .collect();
    assert_eq!(arcs.len(), 3, "{arcs:?}");
    // The flange stays.
    assert!(arcs.iter().any(|a| same(a.0, 10.0) && a.2 == Pen::Thick));
    // The crest circle whole and wide.
    assert!(
        arcs.iter()
            .any(|a| same(a.0, MAJOR / 2.0) && (a.1 - TAU).abs() < 1e-9 && a.2 == Pen::Thick),
        "{arcs:?}"
    );
    // The root about three quarters of a circle, narrow.
    let root = arcs
        .iter()
        .find(|a| same(a.0, MINOR / 2.0))
        .unwrap_or_else(|| panic!("{arcs:?}"));
    assert_eq!(root.2, Pen::Thin);
    assert!(root.1 > 0.7 * TAU && root.1 < 0.76 * TAU, "{arcs:?}");
}

#[test]
fn a_cosmetic_thread_in_a_hole_is_drawn_the_same_way() {
    // A plate 20 x 10 (Z up) with a hole tapped M8 through it along Z at
    // x 10: the hole's bore is the thread's minor diameter, no flanks.
    let thread = ThreadInput {
        start: [10.0, 0.0, 10.0],
        end: [10.0, 0.0, 0.0],
        major: MAJOR,
        minor: MINOR,
        internal: true,
        taper: 0.0,
        source: None,
    };
    let (crest, root) = (MINOR / 2.0, MAJOR / 2.0);
    // From the front the hole is hidden: two dashed lines.
    let mut edges = vec![
        line([0.0, 0.0], [20.0, 0.0]),
        line([20.0, 0.0], [20.0, 10.0]),
        line([20.0, 10.0], [0.0, 10.0]),
        line([0.0, 10.0], [0.0, 0.0]),
    ];
    for side in [-1.0, 1.0] {
        edges.push(kind_line(
            ViewEdgeKind::Sharp,
            false,
            [10.0 + side * crest, 0.0],
            [10.0 + side * crest, 10.0],
        ));
    }
    let projection = ViewProjection {
        edges,
        ..ViewProjection::default()
    };
    let drawn = with_threads(&projection, &front(), std::slice::from_ref(&thread)).edges;
    let at_x = |x: f64| {
        drawn
            .iter()
            .filter(|e| match e.curve {
                ViewCurve::Line(a, b) => (a[0] - x).abs() < 1e-6 && (b[0] - x).abs() < 1e-6,
                _ => false,
            })
            .map(|e| edge_pen(e.visible, e.kind))
            .collect::<Vec<_>>()
    };
    for side in [-1.0, 1.0] {
        // The crests (the minor diameter) and the roots, hidden.
        assert_eq!(at_x(10.0 + side * crest), [Pen::Hidden], "{drawn:?}");
        assert_eq!(at_x(10.0 + side * root), [Pen::Hidden], "{drawn:?}");
    }
    let root_kind = drawn
        .iter()
        .filter(|e| e.kind == ViewEdgeKind::ThreadRoot)
        .count();
    assert_eq!(root_kind, 2, "{drawn:?}");
    // The plate's faces end the thread at both ends: no end lines.
    assert_eq!(drawn.len(), 4 + 4, "{drawn:?}");

    // From above: the crest circle (the minor diameter) wide, the root
    // (the major) a narrow three-quarter arc; the plate's outline stays.
    let top = ViewSpec {
        toward_eye: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        ..front()
    };
    let mut edges = vec![
        line([0.0, -5.0], [20.0, -5.0]),
        line([20.0, -5.0], [20.0, 5.0]),
        line([20.0, 5.0], [0.0, 5.0]),
        line([0.0, 5.0], [0.0, -5.0]),
    ];
    edges.push(arc([10.0, 0.0], crest, 0.0, TAU));
    let projection = ViewProjection {
        edges,
        ..ViewProjection::default()
    };
    let drawn = with_threads(&projection, &top, &[thread]).edges;
    assert_eq!(lines(&drawn).len(), 4);
    let arcs: Vec<(f64, f64, Pen)> = drawn
        .iter()
        .filter_map(|e| match e.curve {
            ViewCurve::Arc {
                radius, start, end, ..
            } => Some((radius, end - start, edge_pen(e.visible, e.kind))),
            _ => None,
        })
        .collect();
    assert_eq!(arcs.len(), 2, "{arcs:?}");
    assert!(
        arcs.iter()
            .any(|a| same(a.0, crest) && same(a.1, TAU) && a.2 == Pen::Thick),
        "{arcs:?}"
    );
    assert!(
        arcs.iter()
            .any(|a| same(a.0, root) && a.1 < 0.76 * TAU && a.2 == Pen::Thin),
        "{arcs:?}"
    );
}

/// A circle of radius `r` about the X axis at `x`, as a view shows it.
fn circle_about_x(spec: &ViewSpec, x: f64, r: f64) -> ViewEdge {
    let points = (0..=48)
        .map(|i| {
            let a = f64::from(i) / 48.0 * TAU;
            spec.project([x, r * a.cos(), r * a.sin()])
        })
        .collect();
    polyline(ViewEdgeKind::Sharp, points)
}

#[test]
fn an_isometric_view_of_a_bolt_draws_its_thread_by_convention() {
    // An M8 thread along X from 10 to the bolt's end at 30, seen from the
    // front, right and above: its crest and root helices a polyline per
    // turn, and the circles of the bolt's end and of its shank's end at 0.
    let iso = ViewSpec {
        toward_eye: [FRAC_1_SQRT_3, -FRAC_1_SQRT_3, FRAC_1_SQRT_3],
        x_axis: [FRAC_1_SQRT_2, FRAC_1_SQRT_2, 0.0],
        ..front()
    };
    let mut helices = Vec::new();
    for k in 0..16 {
        for radius in [MAJOR / 2.0, MINOR / 2.0] {
            let points = (0..=24)
                .map(|i| {
                    let a = f64::from(i) / 24.0 * TAU;
                    iso.project([
                        10.0 + PITCH * (f64::from(k) + a / TAU),
                        radius * a.cos(),
                        radius * a.sin(),
                    ])
                })
                .collect();
            helices.push(polyline(ViewEdgeKind::Sharp, points));
        }
    }
    let end = circle_about_x(&iso, 30.0, MAJOR / 2.0);
    let shank = circle_about_x(&iso, 0.0, MAJOR / 2.0);
    let mut edges = helices.clone();
    edges.push(end.clone());
    edges.push(shank.clone());
    let projection = ViewProjection {
        edges,
        ..ViewProjection::default()
    };
    let thread = ThreadInput {
        start: [10.0, 0.0, 0.0],
        end: [30.0, 0.0, 0.0],
        major: MAJOR,
        minor: MINOR,
        internal: false,
        taper: 0.0,
        source: None,
    };
    let drawn = with_threads(&projection, &iso, &[thread]).edges;
    // No helix edges; the faces' circles stay.
    assert!(helices.iter().all(|h| !drawn.contains(h)), "{drawn:?}");
    assert!(drawn.contains(&end) && drawn.contains(&shank), "{drawn:?}");
    // The crests' outlines wide, the roots' narrow, and the thread's end
    // at 10 (no face ends it there) as the half of its crest circle the
    // eye sees.
    let count = |kind: ViewEdgeKind, line: bool| {
        drawn
            .iter()
            .filter(|e| e.kind == kind && matches!(e.curve, ViewCurve::Line(..)) == line)
            .count()
    };
    assert_eq!(count(ViewEdgeKind::Sharp, true), 2, "{drawn:?}");
    assert_eq!(count(ViewEdgeKind::ThreadRoot, true), 2, "{drawn:?}");
    assert_eq!(count(ViewEdgeKind::Sharp, false), 3, "{drawn:?}");
    assert!(drawn.iter().all(|e| e.visible));
}

#[test]
fn a_tapered_threads_crests_follow_its_taper() {
    // An external taper pipe thread (1:16 on the diameter) along X from the
    // shaft's end at 10, where it starts at 8, to 30, seen from the front.
    let taper = 1.0 / 16.0;
    let radius_at = |x: f64| MAJOR / 2.0 + taper / 2.0 * (x - 10.0);
    let mut edges = vec![line([10.0, -4.0], [10.0, 4.0])];
    for k in 0..16 {
        for depth in [0.0, (MAJOR - MINOR) / 2.0] {
            let points = (0..=12)
                .map(|i| {
                    let a = f64::from(i) / 12.0 * std::f64::consts::PI;
                    let x = 10.0 + PITCH * (f64::from(k) + a / TAU);
                    [x, (radius_at(x) - depth) * a.cos()]
                })
                .collect();
            edges.push(polyline(ViewEdgeKind::Sharp, points));
        }
    }
    let thread = ThreadInput {
        start: [10.0, 0.0, 0.0],
        end: [30.0, 0.0, 0.0],
        major: MAJOR,
        minor: MINOR,
        internal: false,
        taper,
        source: None,
    };
    let projection = ViewProjection {
        edges,
        ..ViewProjection::default()
    };
    let drawn = with_threads(&projection, &front(), &[thread]).edges;
    assert!(
        drawn.iter().all(|e| matches!(e.curve, ViewCurve::Line(..))),
        "{drawn:?}"
    );
    // The upper crest line starts at the end's 8 and widens along the
    // axis by 1/32 a side: not parallel to it.
    let crest = lines(&drawn)
        .into_iter()
        .zip(
            drawn
                .iter()
                .filter(|e| matches!(e.curve, ViewCurve::Line(..))),
        )
        .find(|((a, b), e)| e.kind == ViewEdgeKind::Sharp && near(*a, [10.0, 4.0]) && b[0] > 20.0)
        .map(|(l, _)| l)
        .unwrap_or_else(|| panic!("no crest line: {drawn:?}"));
    let (a, b) = crest;
    assert!(b[1] - a[1] > 0.5, "{crest:?}");
    assert!(
        same((b[1] - a[1]) / (b[0] - a[0]), taper / 2.0),
        "{crest:?}"
    );
    // The root line too, a thread's depth inside.
    assert!(
        drawn.iter().any(|e| match e.curve {
            ViewCurve::Line(p, q) => {
                e.kind == ViewEdgeKind::ThreadRoot
                    && near(p, [10.0, MINOR / 2.0])
                    && same((q[1] - p[1]) / (q[0] - p[0]), taper / 2.0)
            }
            _ => false,
        }),
        "{drawn:?}"
    );
}

#[test]
fn a_nut_hides_the_bolts_thread_behind_it() {
    // An M8 bolt along X, threaded from its end at 0 to 30 and its shank on
    // to 40, with a nut 13 across from 10 to 17.5, seen from the front with
    // hidden edges: the thread's helices behind the nut are hidden.
    let (nut_from, nut_to) = (10.0, 17.5);
    let mut edges = vec![
        line([0.0, -4.0], [0.0, 4.0]),
        line([30.0, 4.0], [40.0, 4.0]),
        line([30.0, -4.0], [40.0, -4.0]),
        line([nut_from, 6.5], [nut_to, 6.5]),
        line([nut_from, -6.5], [nut_to, -6.5]),
        line([nut_from, -6.5], [nut_from, 6.5]),
        line([nut_to, -6.5], [nut_to, 6.5]),
    ];
    for k in 0..24 {
        let from = PITCH * f64::from(k);
        let behind = from >= nut_from && from + PITCH / 2.0 <= nut_to;
        for radius in [MAJOR / 2.0, MINOR / 2.0] {
            let points = (0..=12)
                .map(|i| {
                    let a = f64::from(i) / 12.0 * std::f64::consts::PI;
                    [from + PITCH * a / TAU, radius * a.cos()]
                })
                .collect();
            edges.push(ViewEdge {
                visible: !behind,
                kind: ViewEdgeKind::Sharp,
                curve: ViewCurve::Polyline(points),
                source: None,
            });
        }
    }
    let thread = ThreadInput {
        start: [0.0, 0.0, 0.0],
        end: [30.0, 0.0, 0.0],
        major: MAJOR,
        minor: MINOR,
        internal: false,
        taper: 0.0,
        source: None,
    };
    let projection = ViewProjection {
        edges,
        ..ViewProjection::default()
    };
    let drawn = with_threads(&projection, &front(), &[thread]).edges;
    // Whether a wide line at height y runs over x, seen or hidden.
    let over = |y: f64, x: f64, visible: bool| {
        drawn.iter().any(|e| match e.curve {
            ViewCurve::Line(a, b) => {
                e.kind == ViewEdgeKind::Sharp
                    && e.visible == visible
                    && same(a[1], y)
                    && same(b[1], y)
                    && a[0].min(b[0]) <= x
                    && a[0].max(b[0]) >= x
            }
            _ => false,
        })
    };
    let middle = (nut_from + nut_to) / 2.0;
    for side in [-1.0, 1.0] {
        let crest = side * MAJOR / 2.0;
        assert!(!over(crest, middle, true), "{drawn:?}");
        assert!(over(crest, middle, false), "{drawn:?}");
        assert!(over(crest, 5.0, true), "{drawn:?}");
        assert!(over(crest, 25.0, true), "{drawn:?}");
    }
    // The nut's faces stay.
    assert!(has_line(&drawn, [nut_from, -6.5], [nut_from, 6.5]));
    assert!(has_line(&drawn, [nut_to, -6.5], [nut_to, 6.5]));
}

/// An edge of the shape `source`.
fn of(source: usize, edge: ViewEdge) -> ViewEdge {
    ViewEdge {
        source: Some(source),
        ..edge
    }
}

#[test]
fn a_nut_seen_end_on_keeps_its_bore() {
    // An M8 bolt (shape 0) along X with a nut (shape 1) on it, seen from
    // the bolt's end: the bolt's crest circle hidden behind the nut, its
    // end's chamfer circle seen through the bore; the nut's bore (the
    // thread's minor diameter) and its outline.
    let hidden_crest = ViewEdge {
        visible: false,
        ..arc([0.0, 0.0], MAJOR / 2.0, 0.0, TAU)
    };
    let bore = of(1, arc([0.0, 0.0], MINOR / 2.0, 0.0, TAU));
    let outline = of(1, arc([0.0, 0.0], 7.0, 0.0, TAU));
    let edges = vec![
        of(0, hidden_crest),
        of(0, arc([0.0, 0.0], 3.0, 0.0, TAU)),
        bore.clone(),
        outline.clone(),
    ];
    let thread = ThreadInput {
        start: [30.0, 0.0, 0.0],
        end: [10.0, 0.0, 0.0],
        major: MAJOR,
        minor: MINOR,
        internal: false,
        taper: 0.0,
        source: Some(0),
    };
    let projection = ViewProjection {
        edges,
        ..ViewProjection::default()
    };
    let drawn = with_threads(&projection, &right(), &[thread]).edges;
    // The nut's lines stay as they were.
    assert!(drawn.contains(&bore), "{drawn:?}");
    assert!(drawn.contains(&outline), "{drawn:?}");
    // The bolt's crest circle, hidden behind the nut, as such.
    assert!(
        drawn.iter().any(|e| e.kind == ViewEdgeKind::Sharp
            && !e.visible
            && e.source.is_none()
            && matches!(e.curve, ViewCurve::Arc { radius, .. } if same(radius, MAJOR / 2.0))),
        "{drawn:?}"
    );
    // None of the bolt's own circles is left.
    assert!(drawn.iter().all(|e| e.source != Some(0)), "{drawn:?}");
}

#[test]
fn another_bodys_lines_over_a_thread_stay() {
    // The threaded shaft (shape 0) with a pin's end (shape 1, a circle of
    // 1.6 in front of it) inside the thread's outline, seen from the front.
    let pin = of(1, arc([20.0, 1.0], 0.8, 0.0, TAU));
    let mut edges: Vec<ViewEdge> = threaded_shaft().into_iter().map(|e| of(0, e)).collect();
    edges.push(pin.clone());
    let thread = ThreadInput {
        start: [10.0, 0.0, 0.0],
        end: [30.0, 0.0, 0.0],
        major: MAJOR,
        minor: MINOR,
        internal: false,
        taper: 0.0,
        source: Some(0),
    };
    let projection = ViewProjection {
        edges,
        ..ViewProjection::default()
    };
    let drawn = with_threads(&projection, &front(), &[thread]).edges;
    assert!(drawn.contains(&pin), "{drawn:?}");
    // The thread's own geometry is gone all the same.
    assert!(
        drawn
            .iter()
            .all(|e| !matches!(e.curve, ViewCurve::Polyline(..))),
        "{drawn:?}"
    );
    assert_eq!(drawn.len(), 4 + 1 + 4 + 1, "{drawn:?}");
}

const FRAC_1_SQRT_2: f64 = std::f64::consts::FRAC_1_SQRT_2;
const FRAC_1_SQRT_3: f64 = 0.577_350_269_189_625_8;
