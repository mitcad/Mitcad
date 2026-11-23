// SPDX-License-Identifier: MIT
//! Cell Grid's planning on its own (mitcad#2238): counts by edge mode,
//! keep-outs and their margin, grids that line up, each shape's area, and
//! the refusals.

use std::f64::consts::PI;

use super::*;
use crate::ids::EntityUid;
use crate::profile::ProfileSegment;
use crate::topo::SegmentKey;

/// A closed polygon of lines `c<first>`.. through `points`.
fn polygon(first: u32, points: &[[f64; 2]]) -> ProfileLoop {
    let n = points.len() as u32;
    let id = |i: u32| EntityUid(first + (i % n));
    ProfileLoop {
        segments: (0..n)
            .map(|i| ProfileSegment {
                key: SegmentKey::between(id(i), id(i + n - 1), id(i + 1)),
                geometry: SegmentGeometry::Line {
                    start: points[i as usize],
                    end: points[((i + 1) % n) as usize],
                },
            })
            .collect(),
    }
}

fn rectangle(first: u32, min: [f64; 2], max: [f64; 2]) -> ProfileLoop {
    polygon(first, &[min, [max[0], min[1]], max, [min[0], max[1]]])
}

fn circle(id: u32, center: [f64; 2], radius: f64) -> ProfileLoop {
    ProfileLoop {
        segments: vec![ProfileSegment {
            key: SegmentKey::closed(EntityUid(id)),
            geometry: SegmentGeometry::Circle {
                center,
                radius,
                start: 0.0,
            },
        }],
    }
}

fn hexagons(clip: CellClip) -> GridSpec {
    GridSpec {
        shape: CellShape::Hexagon,
        sides: 8,
        rows: CellRows::Staggered,
        size: 8.0,
        web: 2.0,
        angle: 0.0,
        anchor: [0.0, 0.0],
        clip,
        margin: 0.0,
    }
}

/// The 100 x 60 mm rectangle about the origin.
fn plate() -> Vec<ProfileLoop> {
    vec![rectangle(1, [-50.0, -30.0], [50.0, 30.0])]
}

fn count(plan: &Plan, piece: Piece) -> usize {
    plan.cells.iter().filter(|c| c.piece == piece).count()
}

fn corners(cell: &PlannedCell) -> &[[f64; 2]] {
    match &cell.outline {
        Outline::Polygon { points, .. } => points,
        Outline::Circle { .. } => panic!("a polygon cell"),
    }
}

/// Hexagons of size 8 and web 2 (pitch 10) on the origin: cell (i, j) at
/// (8.660 i, 5 i + 10 j), reaching 4.619 along x and 4 along y. Whole
/// cells: |x| <= 45.381 gives the columns -5..5; the even ones have
/// centres at y = 0, ±10, ±20 (5 each), the odd ones at ±5, ±15, ±25 (6
/// each): 5 x 5 + 6 x 6 = 61. Snapped: the even columns -4..4 add the
/// half below (above) the line through the centres at y = 30 (-30), 10
/// halves, and the columns ±6 at x = ±51.96 the corner triangle that ends
/// 0.35 short of x = ±50 at y = 0, ±10, ±20, 10 sixths: 81. Trimmed: every
/// cell that reaches in, also those of the columns ±6 at y = ±30 and the
/// halves' cells: 85.
#[test]
fn a_plate_of_hexagons_counts_by_edge_mode() {
    let whole = plan(&plate(), &hexagons(CellClip::Whole)).unwrap();
    assert_eq!(whole.cells.len(), 61);
    let cell = 32.0 * 3f64.sqrt();
    assert!((whole.cell_area - cell).abs() < 1e-9);
    assert!((whole.open_area - 61.0 * cell).abs() < 1e-6);
    assert!((whole.field_area - 6000.0).abs() < 1e-9);
    assert!((whole.open_fraction() - 61.0 * cell / 6000.0).abs() < 1e-12);

    let snapped = plan(&plate(), &hexagons(CellClip::Snap)).unwrap();
    assert_eq!(snapped.cells.len(), 81);
    assert_eq!(count(&snapped, Piece::Whole), 61);
    assert_eq!(count(&snapped, Piece::Half), 10);
    assert_eq!(count(&snapped, Piece::Sixth), 10);
    assert!((snapped.open_area - (61.0 + 10.0 / 2.0 + 10.0 / 6.0) * cell).abs() < 1e-6);
    // A half's cut lies on the field's edge, along the lattice's line.
    let half = snapped
        .cells
        .iter()
        .find(|c| c.piece == Piece::Half && c.index == [0, 3])
        .expect("the half at (0, 30)");
    assert!(corners(half).iter().all(|p| p[1] <= 30.0 + 1e-12));
    let Outline::Polygon { sides, .. } = &half.outline else {
        unreachable!()
    };
    assert!(sides.contains(&CHORD));

    let trimmed = plan(&plate(), &hexagons(CellClip::Free)).unwrap();
    assert_eq!(trimmed.cells.len(), 85);
    assert_eq!(count(&trimmed, Piece::Whole), 61);
    assert_eq!(count(&trimmed, Piece::Trimmed), 24);
    // A trimmed cell's open area is its part inside: the half at (0, 30)
    // is half a cell, the corner at (51.96, 30) 1.4 + 4.619 mm² (a box and
    // a triangle left of x = 50 and below y = 30).
    let at = |index: [i64; 2]| {
        trimmed
            .cells
            .iter()
            .find(|c| c.index == index)
            .expect("a trimmed cell")
            .area
    };
    assert!((at([0, 3]) - cell / 2.0).abs() < 1e-9);
    let r = 8.0 / 3f64.sqrt();
    let x = 6.0 * 10.0 * 3f64.sqrt() / 2.0;
    let corner = (50.0 - (x - r / 2.0)) * 4.0 + r / 2.0 * 4.0 / 2.0;
    assert!((at([6, 0]) - corner).abs() < 1e-9, "{}", at([6, 0]));
    // Trimmed cells' areas add up to the field's under the grid.
    assert!(trimmed.open_area > snapped.open_area);
    assert!(trimmed.open_area < 6000.0);
}

/// A circle keep-out inside the plate, with a margin: no cell comes within
/// the margin of it or of the plate's edges, whole or snapped, and none
/// lies in it.
#[test]
fn a_circle_keep_out_leaves_no_cell_within_the_margin() {
    let center = [5.0, 3.0];
    let (radius, margin) = (12.0, 1.5);
    let mut field = plate();
    field.push(circle(9, center, radius));
    for clip in [CellClip::Whole, CellClip::Snap] {
        let mut spec = hexagons(clip);
        spec.margin = margin;
        let plan = plan(&field, &spec).unwrap();
        assert!(plan.cells.len() > 20, "{clip:?}: {}", plan.cells.len());
        let field_area = 6000.0 - PI * radius * radius;
        assert!((plan.field_area - field_area).abs() < 0.01);
        for cell in &plan.cells {
            let points = corners(cell);
            let n = points.len();
            let to_circle = (0..n)
                .map(|k| point_segment(center, points[k], points[(k + 1) % n]))
                .fold(f64::INFINITY, f64::min);
            assert!(
                to_circle >= radius + margin - 1e-6,
                "{clip:?}: cell {:?} is {to_circle} from the keep-out's centre",
                cell.index
            );
            for p in points {
                assert!(p[0].abs() <= 50.0 - margin + 1e-6 && p[1].abs() <= 30.0 - margin + 1e-6);
            }
        }
    }
    // Without the margin more cells fit, none in the keep-out.
    let mut spec = hexagons(CellClip::Whole);
    spec.margin = margin;
    let kept = plan(&field, &spec).unwrap().cells.len();
    let all = plan(&field, &hexagons(CellClip::Whole)).unwrap();
    assert!(all.cells.len() > kept);
    for cell in &all.cells {
        let points = corners(cell);
        let n = points.len();
        let to_circle = (0..n)
            .map(|k| point_segment(center, points[k], points[(k + 1) % n]))
            .fold(f64::INFINITY, f64::min);
        assert!(to_circle >= radius - 1e-6, "{:?}", cell.index);
    }
}

/// Picking the region with the hole and the disc filling it is the same
/// field: the disc's loop is the hole's, a keep-out still.
#[test]
fn a_loop_listed_twice_is_one_keep_out() {
    let mut once = plate();
    once.push(circle(9, [0.0, 0.0], 10.0));
    let mut twice = once.clone();
    twice.push(circle(9, [0.0, 0.0], 10.0));
    let a = plan(&once, &hexagons(CellClip::Whole)).unwrap();
    let b = plan(&twice, &hexagons(CellClip::Whole)).unwrap();
    assert_eq!(a.cells, b.cells);
    // A loop inside the keep-out is a field again.
    let mut nested = once.clone();
    nested.push(rectangle(20, [-6.0, -6.0], [6.0, 6.0]));
    let c = plan(&nested, &hexagons(CellClip::Whole)).unwrap();
    assert!(c.cells.iter().any(|cell| cell.index == [0, 0]));
    assert!(!a.cells.iter().any(|cell| cell.index == [0, 0]));
}

/// Two fields with the same anchor, angle, size and web share one lattice:
/// each half of the plate gets the cells the whole plate has there, by the
/// same index and outline.
#[test]
fn two_fields_with_the_same_anchor_and_angle_line_up() {
    for shape in [
        CellShape::Hexagon,
        CellShape::Square,
        CellShape::Triangle,
        CellShape::Circle,
    ] {
        let mut spec = hexagons(CellClip::Whole);
        spec.shape = shape;
        spec.anchor = [3.0, 7.0];
        spec.angle = 0.3;
        let whole = plan(&plate(), &spec).unwrap();
        for half in [
            rectangle(1, [-50.0, -30.0], [0.0, 30.0]),
            rectangle(5, [0.0, -30.0], [50.0, 30.0]),
        ] {
            let part = plan(&[half], &spec).unwrap();
            assert!(!part.cells.is_empty());
            for cell in &part.cells {
                let same = whole
                    .cells
                    .iter()
                    .find(|c| c.index == cell.index)
                    .unwrap_or_else(|| panic!("{shape:?}: cell {:?} of the whole", cell.index));
                match (&same.outline, &cell.outline) {
                    (Outline::Polygon { points: a, .. }, Outline::Polygon { points: b, .. }) => {
                        for (p, q) in a.iter().zip(b) {
                            assert!(length(sub(*p, *q)) < 1e-9);
                        }
                    }
                    (Outline::Circle { center: a, .. }, Outline::Circle { center: b, .. }) => {
                        assert!(length(sub(*a, *b)) < 1e-9)
                    }
                    _ => panic!("{shape:?}: outlines of two kinds"),
                }
            }
        }
        // The anchor holds cell (0, 0)'s centre.
        let centre = Lattice::new(&spec).outline(0, 0, &spec).inner_point();
        assert!(length(sub(centre, spec.anchor)) < 1e-9, "{shape:?}");
    }
}

/// Each shape's cell has the area of its formula, n r² tan(pi / n) for a
/// regular polygon of inscribed radius r and pi r² for a circle, the
/// planned outlines too.
#[test]
fn each_shapes_cell_area_matches_its_formula() {
    let r: f64 = 3.0;
    let regular = |n: f64| n * r * r * (PI / n).tan();
    let cases = [
        (CellShape::Hexagon, 6, regular(6.0)),
        (CellShape::Square, 4, 4.0 * r * r),
        (CellShape::Triangle, 3, 3.0 * 3f64.sqrt() * r * r),
        (CellShape::Circle, 0, PI * r * r),
        (CellShape::Polygon, 5, regular(5.0)),
        (CellShape::Polygon, 8, regular(8.0)),
        (CellShape::Polygon, 13, regular(13.0)),
    ];
    for (shape, sides, area) in cases {
        let spec = GridSpec {
            shape,
            sides,
            rows: CellRows::Staggered,
            size: 2.0 * r,
            web: 1.0,
            angle: 0.2,
            anchor: [0.5, -0.25],
            clip: CellClip::Whole,
            margin: 0.0,
        };
        let plan = plan(&[rectangle(1, [-30.0, -30.0], [30.0, 30.0])], &spec).unwrap();
        assert!((plan.cell_area - area).abs() < 1e-6, "{shape:?} {sides}");
        for cell in &plan.cells {
            assert!(
                (cell.outline.area() - area).abs() < 1e-6,
                "{shape:?} {sides}"
            );
            assert!((cell.area - area).abs() < 1e-6);
        }
        assert!(plan.cells.len() > 10, "{shape:?}: {}", plan.cells.len());
        // Neighbouring cells keep the web between them.
        for (k, a) in plan.cells.iter().enumerate() {
            for b in &plan.cells[k + 1..] {
                let gap = gap_between(&a.outline, &b.outline);
                assert!(
                    gap >= 1.0 - 1e-9,
                    "{shape:?} {sides}: {:?} {:?} {gap}",
                    a.index,
                    b.index
                );
            }
        }
    }
}

/// The least distance between two cells' outlines.
fn gap_between(a: &Outline, b: &Outline) -> f64 {
    match (a, b) {
        (
            Outline::Circle {
                center: c,
                radius: r,
            },
            Outline::Circle {
                center: d,
                radius: s,
            },
        ) => length(sub(*c, *d)) - r - s,
        (Outline::Polygon { points: p, .. }, Outline::Polygon { points: q, .. }) => {
            let (n, m) = (p.len(), q.len());
            (0..n)
                .flat_map(|i| {
                    (0..m).map(move |k| segment_segment(p[i], p[(i + 1) % n], q[k], q[(k + 1) % m]))
                })
                .fold(f64::INFINITY, f64::min)
        }
        _ => unreachable!(),
    }
}

/// Square rows of circles: the pitch is the size plus the web both ways.
#[test]
fn circles_in_square_rows_lie_on_a_square_lattice() {
    let mut spec = hexagons(CellClip::Whole);
    spec.shape = CellShape::Circle;
    spec.rows = CellRows::Square;
    let plan = plan(&plate(), &spec).unwrap();
    // Centres at 10 i, 10 j with |x| <= 46 and |y| <= 26: 9 x 5.
    assert_eq!(plan.cells.len(), 45);
}

#[test]
fn the_refusals_are_in_words() {
    let mut spec = hexagons(CellClip::Whole);
    spec.web = 0.0;
    assert_eq!(
        plan(&plate(), &spec).unwrap_err(),
        "the web between cells must be greater than zero, got 0"
    );
    let mut spec = hexagons(CellClip::Whole);
    spec.size = 70.0;
    assert!(
        plan(&plate(), &spec)
            .unwrap_err()
            .starts_with("no cell of size 70 mm fits in the field")
    );
    // Too small for the field: about 6000 / 2.6 cells.
    let mut spec = hexagons(CellClip::Whole);
    spec.size = 1.0;
    spec.web = 0.5;
    let error = plan(&plate(), &spec).unwrap_err();
    assert!(
        error.starts_with("the grid would have about ")
            && error.contains("cells, more than the 250 one Cell Grid cuts"),
        "{error}"
    );
    // Over the cap by a few: counted.
    let mut spec = hexagons(CellClip::Whole);
    spec.size = 4.0;
    spec.web = 1.0;
    let error = plan(&plate(), &spec).unwrap_err();
    assert!(error.starts_with("the grid has "), "{error}");
    let mut spec = hexagons(CellClip::Whole);
    spec.shape = CellShape::Polygon;
    spec.sides = 2;
    assert!(plan(&plate(), &spec).unwrap_err().contains("3 to 64 sides"));
    let mut spec = hexagons(CellClip::Free);
    spec.margin = 1.0;
    assert!(
        plan(&plate(), &spec)
            .unwrap_err()
            .contains("keep no margin")
    );
    // A loop whose lines do not meet.
    let mut open = rectangle(1, [-50.0, -30.0], [50.0, 30.0]);
    if let SegmentGeometry::Line { end, .. } = &mut open.segments[1].geometry {
        end[1] += 0.5;
    }
    let error = plan(&[open], &hexagons(CellClip::Whole)).unwrap_err();
    assert!(error.contains("is not closed"), "{error}");
}
