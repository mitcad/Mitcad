// SPDX-License-Identifier: MIT
//! Reader tests with hand-written DXF snippets.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use mitcad_dxf::{Drawing, Geometry, Point2, Units, read, read_str};

/// DXF text of group pairs.
fn dxf(pairs: &[(i32, &str)]) -> String {
    pairs
        .iter()
        .map(|(code, value)| format!("{code}\n{value}\n"))
        .collect()
}

/// A file with the given entity groups in its ENTITIES section.
fn entities(groups: &[(i32, &str)]) -> Drawing {
    let mut pairs = vec![(0, "SECTION"), (2, "ENTITIES")];
    pairs.extend_from_slice(groups);
    pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
    read_str(&dxf(&pairs)).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn near_point(a: Point2, b: Point2) -> bool {
    near(a.x, b.x) && near(a.y, b.y)
}

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

#[track_caller]
fn assert_line(g: &Geometry, start: Point2, end: Point2) {
    match *g {
        Geometry::Line { start: s, end: e } => {
            assert!(
                near_point(s, start) && near_point(e, end),
                "line {s:?}-{e:?}, expected {start:?}-{end:?}"
            )
        }
        ref other => panic!("expected a line, got {other:?}"),
    }
}

#[track_caller]
fn assert_arc(g: &Geometry, center: Point2, radius: f64, start: f64, end: f64) {
    match *g {
        Geometry::Arc {
            center: c,
            radius: r,
            start_angle: s,
            end_angle: e,
        } => assert!(
            near_point(c, center) && near(r, radius) && near(s, start) && near(e, end),
            "arc {c:?} r {r} {s}..{e}, expected {center:?} r {radius} {start}..{end}"
        ),
        ref other => panic!("expected an arc, got {other:?}"),
    }
}

#[test]
fn basic_entities() {
    let d = entities(&[
        (0, "LINE"),
        (8, "Edges"),
        (10, "1.5"),
        (20, "2"),
        (30, "0"),
        (11, "4"),
        (21, "6"),
        (31, "0"),
        (0, "CIRCLE"),
        (10, "0"),
        (20, "0"),
        (40, "5"),
        (0, "ARC"),
        (10, "1"),
        (20, "1"),
        (40, "2"),
        (50, "90"),
        (51, "0"),
        (0, "POINT"),
        (10, "-3"),
        (20, "7.25"),
    ]);
    assert_eq!(d.entities.len(), 4);
    assert_eq!(d.entities[0].layer, "Edges");
    assert_eq!(d.entities[1].layer, "0");
    assert_line(&d.entities[0].geometry, p(1.5, 2.0), p(4.0, 6.0));
    assert_eq!(
        d.entities[1].geometry,
        Geometry::Circle {
            center: p(0.0, 0.0),
            radius: 5.0
        }
    );
    // 90 to 0 degrees counter-clockwise is a 270 degree arc.
    assert_arc(&d.entities[2].geometry, p(1.0, 1.0), 2.0, FRAC_PI_2, TAU);
    assert_eq!(d.entities[3].geometry, Geometry::Point(p(-3.0, 7.25)));
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
    assert_eq!(d.units, Units::Unitless);
}

#[test]
fn lwpolyline_with_bulges() {
    // A 10 x 10 slot: straight bottom and top, half circles at the ends.
    let d = entities(&[
        (0, "LWPOLYLINE"),
        (8, "Slot"),
        (90, "4"),
        (70, "1"),
        (10, "0"),
        (20, "0"),
        (10, "10"),
        (20, "0"),
        (42, "1"),
        (10, "10"),
        (20, "10"),
        (10, "0"),
        (20, "10"),
        (42, "1.0"),
    ]);
    let g: Vec<&Geometry> = d.entities.iter().map(|e| &e.geometry).collect();
    assert_eq!(g.len(), 4);
    assert_line(g[0], p(0.0, 0.0), p(10.0, 0.0));
    assert_arc(g[1], p(10.0, 5.0), 5.0, 1.5 * PI, 2.5 * PI);
    assert_line(g[2], p(10.0, 10.0), p(0.0, 10.0));
    assert_arc(g[3], p(0.0, 5.0), 5.0, FRAC_PI_2, 1.5 * PI);
    assert!(d.entities.iter().all(|e| e.layer == "Slot"));
}

#[test]
fn lwpolyline_negative_bulge_and_open() {
    let b = format!("{}", -(PI / 8.0).tan());
    let d = entities(&[
        (0, "LWPOLYLINE"),
        (90, "3"),
        (70, "0"),
        (10, "1"),
        (20, "0"),
        (42, &b),
        (10, "0"),
        (20, "1"),
        (10, "0"),
        (20, "5"),
    ]);
    assert_eq!(d.entities.len(), 2);
    assert_arc(&d.entities[0].geometry, p(1.0, 1.0), 1.0, PI, 1.5 * PI);
    assert_line(&d.entities[1].geometry, p(0.0, 1.0), p(0.0, 5.0));
}

#[test]
fn r12_polyline_with_vertices() {
    let d = entities(&[
        (0, "POLYLINE"),
        (8, "P"),
        (66, "1"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (70, "1"),
        (0, "VERTEX"),
        (8, "P"),
        (10, "0"),
        (20, "0"),
        (0, "VERTEX"),
        (8, "P"),
        (10, "4"),
        (20, "0"),
        (42, "1"),
        (0, "VERTEX"),
        (8, "P"),
        (10, "4"),
        (20, "4"),
        (0, "SEQEND"),
        (8, "P"),
        (0, "LINE"),
        (10, "9"),
        (20, "9"),
        (11, "10"),
        (21, "10"),
    ]);
    let g: Vec<&Geometry> = d.entities.iter().map(|e| &e.geometry).collect();
    assert_eq!(g.len(), 4, "{g:?}");
    assert_line(g[0], p(0.0, 0.0), p(4.0, 0.0));
    assert_arc(g[1], p(4.0, 2.0), 2.0, 1.5 * PI, 2.5 * PI);
    assert_line(g[2], p(4.0, 4.0), p(0.0, 0.0));
    assert_line(g[3], p(9.0, 9.0), p(10.0, 10.0));
}

#[test]
fn polyline_meshes_are_skipped() {
    let d = entities(&[
        (0, "POLYLINE"),
        (66, "1"),
        (70, "64"),
        (0, "VERTEX"),
        (10, "0"),
        (20, "0"),
        (0, "SEQEND"),
        (0, "DIMENSION"),
        (0, "HATCH"),
        (0, "HATCH"),
    ]);
    assert!(d.entities.is_empty());
    assert!(
        d.warnings.iter().any(|w| w == "skipped 2 HATCH entities"),
        "{:?}",
        d.warnings
    );
    assert!(
        d.warnings
            .iter()
            .any(|w| w == "skipped 1 DIMENSION entities")
    );
    assert!(
        d.warnings
            .iter()
            .any(|w| w == "skipped 1 POLYLINE mesh entities")
    );
}

#[test]
fn ellipses() {
    let d = entities(&[
        (0, "ELLIPSE"),
        (10, "1"),
        (20, "2"),
        (30, "0"),
        (11, "0"),
        (21, "4"),
        (31, "0"),
        (40, "0.5"),
        (41, "0"),
        (42, "6.283185307179586"),
        // Upside-down extrusion: the parameter runs clockwise seen from +Z.
        (0, "ELLIPSE"),
        (10, "0"),
        (20, "0"),
        (11, "2"),
        (21, "0"),
        (40, "0.5"),
        (41, "0"),
        (42, "1.5707963267948966"),
        (210, "0"),
        (220, "0"),
        (230, "-1"),
    ]);
    match d.entities[0].geometry {
        Geometry::Ellipse {
            center,
            major_axis,
            ratio,
            start_param,
            end_param,
        } => {
            assert!(near_point(center, p(1.0, 2.0)));
            assert!(near_point(major_axis, p(0.0, 4.0)));
            assert!(near(ratio, 0.5));
            assert!(near(start_param, 0.0) && near(end_param, TAU));
        }
        ref other => panic!("{other:?}"),
    }
    // Clockwise from (2, 0) to (0, -1): counter-clockwise from (0, -1) to (2, 0).
    match d.entities[1].geometry {
        Geometry::Ellipse {
            center,
            major_axis,
            ratio,
            start_param,
            end_param,
        } => {
            assert!(near_point(center, p(0.0, 0.0)));
            assert!(near(ratio, 0.5));
            let at = |t: f64| center + major_axis * t.cos() + major_axis.perp() * (ratio * t.sin());
            assert!(
                near_point(at(start_param), p(0.0, -1.0)),
                "{:?}",
                at(start_param)
            );
            assert!(
                near_point(at(end_param), p(2.0, 0.0)),
                "{:?}",
                at(end_param)
            );
            assert!(near(end_param - start_param, FRAC_PI_2));
            // The midpoint lies in the fourth quadrant.
            let mid = at(0.5 * (start_param + end_param));
            assert!(mid.x > 0.0 && mid.y < 0.0);
        }
        ref other => panic!("{other:?}"),
    }
}

#[test]
fn arc_with_flipped_extrusion() {
    // OCS x is world -x for extrusion (0, 0, -1).
    let d = entities(&[
        (0, "ARC"),
        (10, "5"),
        (20, "0"),
        (30, "0"),
        (40, "1"),
        (50, "0"),
        (51, "90"),
        (210, "0"),
        (220, "0"),
        (230, "-1"),
        (0, "CIRCLE"),
        (10, "3"),
        (20, "1"),
        (40, "1"),
        (210, "0"),
        (220, "0"),
        (230, "-1"),
    ]);
    // OCS arc from (6, 0) to (5, 1) maps to world (-6, 0) .. (-5, 1),
    // counter-clockwise from (-5, 1) to (-6, 0) around (-5, 0).
    assert_arc(&d.entities[0].geometry, p(-5.0, 0.0), 1.0, FRAC_PI_2, PI);
    assert_eq!(
        d.entities[1].geometry,
        Geometry::Circle {
            center: p(-3.0, 1.0),
            radius: 1.0
        }
    );
}

#[test]
fn tilted_circle_projects_to_an_ellipse() {
    let s = format!("{}", 0.5_f64.sqrt());
    let d = entities(&[
        (0, "CIRCLE"),
        (10, "0"),
        (20, "0"),
        (40, "2"),
        (210, &s),
        (220, "0"),
        (230, &s),
    ]);
    match d.entities[0].geometry {
        Geometry::Ellipse {
            major_axis, ratio, ..
        } => {
            assert!(near(major_axis.length(), 2.0));
            assert!(near(ratio, 0.5_f64.sqrt()));
        }
        ref other => panic!("{other:?}"),
    }
    let d = entities(&[
        (0, "CIRCLE"),
        (10, "0"),
        (20, "0"),
        (40, "2"),
        (210, "1"),
        (220, "0"),
        (230, "0"),
    ]);
    assert!(d.entities.is_empty());
    assert!(d.warnings[0].contains("perpendicular"));
}

#[test]
fn splines() {
    let d = entities(&[
        (0, "SPLINE"),
        (8, "S"),
        (70, "12"),
        (71, "2"),
        (72, "6"),
        (73, "3"),
        (74, "0"),
        (40, "0"),
        (40, "0"),
        (40, "0"),
        (40, "1"),
        (40, "1"),
        (40, "1"),
        (41, "1"),
        (41, "0.7071067811865476"),
        (41, "1"),
        (10, "1"),
        (20, "0"),
        (30, "0"),
        (10, "1"),
        (20, "1"),
        (30, "0"),
        (10, "0"),
        (20, "1"),
        (30, "0"),
        // Fit points only.
        (0, "SPLINE"),
        (70, "8"),
        (71, "3"),
        (72, "0"),
        (73, "0"),
        (74, "3"),
        (11, "0"),
        (21, "0"),
        (11, "1"),
        (21, "2"),
        (11, "3"),
        (21, "1"),
    ]);
    match &d.entities[0].geometry {
        Geometry::Spline(s) => {
            assert_eq!(s.degree, 2);
            assert_eq!(s.control_points.len(), 3);
            assert_eq!(s.knots, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
            assert!(s.is_rational());
            assert!(!s.closed);
            // A rational quadratic quarter circle: every point at radius 1.
            for i in 0..=10 {
                let q = s.point_at(i as f64 / 10.0).unwrap();
                assert!(near(q.length(), 1.0), "{q:?}");
            }
        }
        other => panic!("{other:?}"),
    }
    match &d.entities[1].geometry {
        Geometry::Spline(s) => {
            assert!(s.control_points.is_empty());
            assert_eq!(s.fit_points, vec![p(0.0, 0.0), p(1.0, 2.0), p(3.0, 1.0)]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn text_and_mtext() {
    let d = entities(&[
        (0, "TEXT"),
        (10, "1"),
        (20, "2"),
        (40, "3.5"),
        (1, "Diameter %%c10 \\U+00E4"),
        (50, "90"),
        (0, "MTEXT"),
        (10, "0"),
        (20, "0"),
        (40, "2"),
        (3, "{\\fArial|b0|i0;First line\\P"),
        (1, "second}"),
        (11, "0"),
        (21, "1"),
        (31, "0"),
    ]);
    match &d.entities[0].geometry {
        Geometry::Text(t) => {
            assert_eq!(t.content, "Diameter \u{2300}10 \u{E4}");
            assert!(near_point(t.position, p(1.0, 2.0)));
            assert!(near(t.height, 3.5));
            assert!(near(t.rotation, FRAC_PI_2));
        }
        other => panic!("{other:?}"),
    }
    match &d.entities[1].geometry {
        Geometry::Text(t) => {
            assert_eq!(t.content, "First line\nsecond");
            assert!(near(t.rotation, FRAC_PI_2));
        }
        other => panic!("{other:?}"),
    }
}

/// A file with one block definition and the given model space groups.
fn with_blocks(blocks: &[(i32, &str)], model: &[(i32, &str)]) -> Drawing {
    let mut pairs = vec![(0, "SECTION"), (2, "BLOCKS")];
    pairs.extend_from_slice(blocks);
    pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
    pairs.extend_from_slice(model);
    pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
    read_str(&dxf(&pairs)).unwrap()
}

/// Block BOLT with base point (1, 1): a line from the base point to (3, 1)
/// on layer 0 and a circle at (1, 1) on its own layer.
const BOLT: &[(i32, &str)] = &[
    (0, "BLOCK"),
    (8, "0"),
    (2, "BOLT"),
    (70, "0"),
    (10, "1"),
    (20, "1"),
    (30, "0"),
    (0, "LINE"),
    (8, "0"),
    (10, "1"),
    (20, "1"),
    (11, "3"),
    (21, "1"),
    (0, "CIRCLE"),
    (8, "Holes"),
    (10, "1"),
    (20, "1"),
    (40, "0.5"),
    (0, "ENDBLK"),
];

#[test]
fn insert_with_rotation_and_scale() {
    let d = with_blocks(
        BOLT,
        &[
            (0, "INSERT"),
            (8, "Parts"),
            (2, "bolt"),
            (10, "100"),
            (20, "0"),
            (41, "2"),
            (42, "2"),
            (50, "90"),
        ],
    );
    assert_eq!(d.entities.len(), 2, "{:?}", d.warnings);
    // Base point to the insertion point, x axis turned to +y, doubled.
    assert_line(&d.entities[0].geometry, p(100.0, 0.0), p(100.0, 4.0));
    assert_eq!(d.entities[0].layer, "Parts");
    assert_eq!(d.entities[1].layer, "Holes");
    match d.entities[1].geometry {
        Geometry::Circle { center, radius } => {
            assert!(near_point(center, p(100.0, 0.0)));
            assert!(near(radius, 1.0));
        }
        ref other => panic!("{other:?}"),
    }
}

#[test]
fn insert_non_uniform_scale_and_mirror() {
    let d = with_blocks(
        BOLT,
        &[
            (0, "INSERT"),
            (2, "BOLT"),
            (10, "0"),
            (20, "0"),
            (41, "1"),
            (42, "3"),
            (0, "INSERT"),
            (2, "BOLT"),
            (10, "0"),
            (20, "0"),
            (41, "-1"),
        ],
    );
    assert_eq!(d.entities.len(), 4);
    match d.entities[1].geometry {
        Geometry::Ellipse {
            center,
            major_axis,
            ratio,
            ..
        } => {
            assert!(near_point(center, p(0.0, 0.0)));
            assert!(near(major_axis.length(), 1.5));
            assert!(near(ratio, 1.0 / 3.0));
        }
        ref other => panic!("{other:?}"),
    }
    assert_line(&d.entities[2].geometry, p(0.0, 0.0), p(-2.0, 0.0));
}

#[test]
fn mirrored_block_arc_stays_counter_clockwise() {
    let block: &[(i32, &str)] = &[
        (0, "BLOCK"),
        (2, "A"),
        (10, "0"),
        (20, "0"),
        (0, "ARC"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
        (50, "0"),
        (51, "45"),
        (0, "ENDBLK"),
    ];
    let d = with_blocks(
        block,
        &[(0, "INSERT"), (2, "A"), (10, "10"), (20, "0"), (41, "-1")],
    );
    // Mirrored about the insertion point's y axis: angles 135 to 180 degrees.
    assert_arc(&d.entities[0].geometry, p(10.0, 0.0), 1.0, 0.75 * PI, PI);
}

#[test]
fn nested_blocks_and_arrays() {
    let mut blocks = BOLT.to_vec();
    blocks.extend_from_slice(&[
        (0, "BLOCK"),
        (8, "0"),
        (2, "PLATE"),
        (10, "0"),
        (20, "0"),
        (0, "INSERT"),
        (8, "0"),
        (2, "BOLT"),
        (10, "10"),
        (20, "0"),
        (0, "ENDBLK"),
    ]);
    let d = with_blocks(
        &blocks,
        &[
            (0, "INSERT"),
            (8, "Assembly"),
            (2, "PLATE"),
            (10, "0"),
            (20, "100"),
            // A 3 x 2 array.
            (70, "3"),
            (71, "2"),
            (44, "50"),
            (45, "20"),
        ],
    );
    assert_eq!(d.entities.len(), 12);
    // Element (column 2, row 1): plate at (100, 120), bolt base at (110, 120).
    assert_line(&d.entities[10].geometry, p(110.0, 120.0), p(112.0, 120.0));
    assert_eq!(d.entities[10].layer, "Assembly");
    assert_eq!(d.entities[11].layer, "Holes");
}

#[test]
fn missing_and_cyclic_blocks_are_reported() {
    let blocks: &[(i32, &str)] = &[
        (0, "BLOCK"),
        (2, "LOOP"),
        (10, "0"),
        (20, "0"),
        (0, "LINE"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "0"),
        (0, "INSERT"),
        (2, "LOOP"),
        (10, "1"),
        (20, "0"),
        (0, "ENDBLK"),
    ];
    let d = with_blocks(
        blocks,
        &[(0, "INSERT"), (2, "LOOP"), (0, "INSERT"), (2, "NOPE")],
    );
    assert_eq!(d.entities.len(), 1);
    assert_eq!(d.warnings.len(), 2, "{:?}", d.warnings);
}

#[test]
fn header_units_and_layers() {
    let text = dxf(&[
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1015"),
        (9, "$INSUNITS"),
        (70, "1"),
        (9, "$EXTMIN"),
        (10, "0"),
        (20, "0"),
        (30, "0"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "LAYER"),
        (70, "2"),
        (0, "LAYER"),
        (2, "Visible"),
        (70, "0"),
        (62, "1"),
        (6, "CONTINUOUS"),
        (0, "LAYER"),
        (2, "Hidden"),
        (70, "1"),
        (62, "-5"),
        (6, "DASHED"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (8, "Visible"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "2"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let d = read_str(&text).unwrap();
    assert_eq!(d.version.as_deref(), Some("AC1015"));
    assert_eq!(d.units, Units::Inches);
    assert_eq!(d.layers.len(), 2);
    let hidden = d.layer("Hidden").unwrap();
    assert!(!hidden.visible && hidden.frozen);
    assert_eq!(hidden.color, 5);
    assert_eq!(hidden.linetype, "DASHED");
    let mm = d.to_millimeters(Units::Millimeters);
    assert_eq!(mm.units, Units::Millimeters);
    assert_line(&mm.entities[0].geometry, p(0.0, 0.0), p(25.4, 50.8));
}

#[test]
fn unitless_files_take_the_given_units() {
    let d = entities(&[(0, "CIRCLE"), (10, "1"), (20, "0"), (40, "1")]);
    let mm = d.to_millimeters(Units::Centimeters);
    assert_eq!(
        mm.entities[0].geometry,
        Geometry::Circle {
            center: p(10.0, 0.0),
            radius: 10.0
        }
    );
}

#[test]
fn paper_space_is_skipped() {
    let d = entities(&[
        (0, "LINE"),
        (67, "1"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "0"),
    ]);
    assert!(d.entities.is_empty());
    assert_eq!(
        d.warnings,
        vec!["skipped 1 paper space entities".to_owned()]
    );
}

#[test]
fn windows_1252_text_and_crlf() {
    let mut bytes = b"  0\r\nSECTION\r\n  2\r\nENTITIES\r\n  0\r\nTEXT\r\n 10\r\n0\r\n 20\r\n0\r\n 40\r\n1\r\n  1\r\n".to_vec();
    bytes.extend_from_slice(b"Br\xfccke\r\n  0\r\nENDSEC\r\n  0\r\nEOF\r\n");
    let d = read(&bytes).unwrap();
    match &d.entities[0].geometry {
        Geometry::Text(t) => assert_eq!(t.content, "Br\u{FC}cke"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn malformed_numbers_skip_the_entity() {
    let d = entities(&[
        (0, "CIRCLE"),
        (10, "zero"),
        (20, "0"),
        (40, "1"),
        (0, "POINT"),
        (10, "1"),
        (20, "1"),
    ]);
    assert_eq!(d.entities.len(), 1);
    assert!(d.warnings[0].contains("CIRCLE skipped"), "{:?}", d.warnings);
}

/// A sketch as CAD applications export it: a DXF 2000 or later
/// file with handles, subclass markers, classes and objects, CRLF lines, millimetres.
const CAD_EXPORT_STYLE: &str = "  0\r\nSECTION\r\n  2\r\nHEADER\r\n  9\r\n$ACADVER\r\n  1\r\nAC1018\r\n  9\r\n$ACADMAINTVER\r\n 70\r\n     0\r\n  9\r\n$DWGCODEPAGE\r\n  3\r\nANSI_1252\r\n  9\r\n$INSBASE\r\n 10\r\n0.0\r\n 20\r\n0.0\r\n 30\r\n0.0\r\n  9\r\n$EXTMIN\r\n 10\r\n-25.0\r\n 20\r\n-10.0\r\n 30\r\n0.0\r\n  9\r\n$EXTMAX\r\n 10\r\n25.0\r\n 20\r\n10.0\r\n 30\r\n0.0\r\n  9\r\n$INSUNITS\r\n 70\r\n     4\r\n  9\r\n$MEASUREMENT\r\n 70\r\n     1\r\n  9\r\n$HANDSEED\r\n  5\r\n30\r\n  0\r\nENDSEC\r\n  0\r\nSECTION\r\n  2\r\nCLASSES\r\n  0\r\nCLASS\r\n  1\r\nACDBDICTIONARYWDFLT\r\n  2\r\nAcDbDictionaryWithDefault\r\n  3\r\nObjectDBX Classes\r\n 90\r\n        0\r\n280\r\n     0\r\n281\r\n     0\r\n  0\r\nENDSEC\r\n  0\r\nSECTION\r\n  2\r\nTABLES\r\n  0\r\nTABLE\r\n  2\r\nVPORT\r\n  5\r\n8\r\n330\r\n0\r\n100\r\nAcDbSymbolTable\r\n 70\r\n     1\r\n  0\r\nVPORT\r\n  5\r\n29\r\n330\r\n8\r\n100\r\nAcDbSymbolTableRecord\r\n100\r\nAcDbViewportTableRecord\r\n  2\r\n*Active\r\n 70\r\n     0\r\n 10\r\n0.0\r\n 20\r\n0.0\r\n 11\r\n1.0\r\n 21\r\n1.0\r\n  0\r\nENDTAB\r\n  0\r\nTABLE\r\n  2\r\nLAYER\r\n  5\r\n2\r\n330\r\n0\r\n100\r\nAcDbSymbolTable\r\n 70\r\n     2\r\n  0\r\nLAYER\r\n  5\r\n10\r\n330\r\n2\r\n100\r\nAcDbSymbolTableRecord\r\n100\r\nAcDbLayerTableRecord\r\n  2\r\n0\r\n 70\r\n     0\r\n 62\r\n     7\r\n  6\r\nContinuous\r\n370\r\n    -3\r\n390\r\nF\r\n  0\r\nLAYER\r\n  5\r\n11\r\n330\r\n2\r\n100\r\nAcDbSymbolTableRecord\r\n100\r\nAcDbLayerTableRecord\r\n  2\r\nSketch1\r\n 70\r\n     0\r\n 62\r\n     5\r\n  6\r\nContinuous\r\n  0\r\nENDTAB\r\n  0\r\nTABLE\r\n  2\r\nBLOCK_RECORD\r\n  5\r\n1\r\n330\r\n0\r\n100\r\nAcDbSymbolTable\r\n 70\r\n     2\r\n  0\r\nBLOCK_RECORD\r\n  5\r\n1F\r\n330\r\n1\r\n100\r\nAcDbSymbolTableRecord\r\n100\r\nAcDbBlockTableRecord\r\n  2\r\n*Model_Space\r\n340\r\n22\r\n  0\r\nBLOCK_RECORD\r\n  5\r\n1B\r\n330\r\n1\r\n100\r\nAcDbSymbolTableRecord\r\n100\r\nAcDbBlockTableRecord\r\n  2\r\n*Paper_Space\r\n340\r\n1E\r\n  0\r\nENDTAB\r\n  0\r\nENDSEC\r\n  0\r\nSECTION\r\n  2\r\nBLOCKS\r\n  0\r\nBLOCK\r\n  5\r\n20\r\n330\r\n1F\r\n100\r\nAcDbEntity\r\n  8\r\n0\r\n100\r\nAcDbBlockBegin\r\n  2\r\n*Model_Space\r\n 70\r\n     0\r\n 10\r\n0.0\r\n 20\r\n0.0\r\n 30\r\n0.0\r\n  3\r\n*Model_Space\r\n  1\r\n\r\n  0\r\nENDBLK\r\n  5\r\n21\r\n330\r\n1F\r\n100\r\nAcDbEntity\r\n  8\r\n0\r\n100\r\nAcDbBlockEnd\r\n  0\r\nBLOCK\r\n  5\r\n1C\r\n330\r\n1B\r\n100\r\nAcDbEntity\r\n 67\r\n     1\r\n  8\r\n0\r\n100\r\nAcDbBlockBegin\r\n  2\r\n*Paper_Space\r\n 70\r\n     0\r\n 10\r\n0.0\r\n 20\r\n0.0\r\n 30\r\n0.0\r\n  3\r\n*Paper_Space\r\n  1\r\n\r\n  0\r\nENDBLK\r\n  5\r\n1D\r\n330\r\n1B\r\n100\r\nAcDbEntity\r\n 67\r\n     1\r\n  8\r\n0\r\n100\r\nAcDbBlockEnd\r\n  0\r\nENDSEC\r\n  0\r\nSECTION\r\n  2\r\nENTITIES\r\n  0\r\nLWPOLYLINE\r\n  5\r\n2A\r\n330\r\n1F\r\n100\r\nAcDbEntity\r\n  8\r\nSketch1\r\n100\r\nAcDbPolyline\r\n 90\r\n        4\r\n 70\r\n     1\r\n 43\r\n0.0\r\n 10\r\n-25.0\r\n 20\r\n-10.0\r\n 10\r\n25.0\r\n 20\r\n-10.0\r\n 10\r\n25.0\r\n 20\r\n10.0\r\n 10\r\n-25.0\r\n 20\r\n10.0\r\n  0\r\nCIRCLE\r\n  5\r\n2B\r\n330\r\n1F\r\n100\r\nAcDbEntity\r\n  8\r\nSketch1\r\n100\r\nAcDbCircle\r\n 10\r\n0.0\r\n 20\r\n0.0\r\n 30\r\n0.0\r\n 40\r\n5.0\r\n  0\r\nARC\r\n  5\r\n2C\r\n330\r\n1F\r\n100\r\nAcDbEntity\r\n  8\r\nSketch1\r\n100\r\nAcDbCircle\r\n 10\r\n15.0\r\n 20\r\n0.0\r\n 30\r\n0.0\r\n 40\r\n3.0\r\n100\r\nAcDbArc\r\n 50\r\n0.0\r\n 51\r\n180.0\r\n  0\r\nSPLINE\r\n  5\r\n2D\r\n330\r\n1F\r\n100\r\nAcDbEntity\r\n  8\r\nSketch1\r\n100\r\nAcDbSpline\r\n210\r\n0.0\r\n220\r\n0.0\r\n230\r\n1.0\r\n 70\r\n     8\r\n 71\r\n     3\r\n 72\r\n     8\r\n 73\r\n     4\r\n 74\r\n     0\r\n 42\r\n0.0000001\r\n 43\r\n0.0000001\r\n 40\r\n0.0\r\n 40\r\n0.0\r\n 40\r\n0.0\r\n 40\r\n0.0\r\n 40\r\n1.0\r\n 40\r\n1.0\r\n 40\r\n1.0\r\n 40\r\n1.0\r\n 10\r\n-20.0\r\n 20\r\n0.0\r\n 30\r\n0.0\r\n 10\r\n-15.0\r\n 20\r\n5.0\r\n 30\r\n0.0\r\n 10\r\n-10.0\r\n 20\r\n-5.0\r\n 30\r\n0.0\r\n 10\r\n-5.0\r\n 20\r\n0.0\r\n 30\r\n0.0\r\n  0\r\nENDSEC\r\n  0\r\nSECTION\r\n  2\r\nOBJECTS\r\n  0\r\nDICTIONARY\r\n  5\r\nC\r\n330\r\n0\r\n100\r\nAcDbDictionary\r\n281\r\n     1\r\n  3\r\nACAD_GROUP\r\n350\r\nD\r\n  0\r\nDICTIONARY\r\n  5\r\nD\r\n330\r\nC\r\n100\r\nAcDbDictionary\r\n281\r\n     1\r\n  0\r\nENDSEC\r\n  0\r\nEOF\r\n";

#[test]
fn cad_export_style_file() {
    let d = read_str(CAD_EXPORT_STYLE).unwrap();
    assert_eq!(d.version.as_deref(), Some("AC1018"));
    assert_eq!(d.units, Units::Millimeters);
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
    assert_eq!(
        d.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
        ["0", "Sketch1"]
    );
    // Rectangle (4 lines), circle, arc and spline.
    assert_eq!(d.entities.len(), 7);
    assert!(d.entities.iter().all(|e| e.layer == "Sketch1"));
    assert_line(&d.entities[0].geometry, p(-25.0, -10.0), p(25.0, -10.0));
    assert_line(&d.entities[3].geometry, p(-25.0, 10.0), p(-25.0, -10.0));
    assert_arc(&d.entities[5].geometry, p(15.0, 0.0), 3.0, 0.0, PI);
    match &d.entities[6].geometry {
        Geometry::Spline(s) => {
            assert_eq!(s.degree, 3);
            assert_eq!(s.control_points.len(), 4);
            assert!(!s.is_rational());
            assert!(near_point(s.point_at(0.0).unwrap(), p(-20.0, 0.0)));
            assert!(near_point(s.point_at(1.0).unwrap(), p(-5.0, 0.0)));
        }
        other => panic!("{other:?}"),
    }
    // Where it lies and what is on which layer (P9: Insert DXF's
    // placement and layers). The arc's top reaches y = 3 and the spline's
    // dip stays inside the rectangle.
    let (lo, hi) = d.bounds().unwrap();
    assert!(
        near_point(lo, p(-25.0, -10.0)) && near_point(hi, p(25.0, 10.0)),
        "{lo:?} {hi:?}"
    );
    assert_eq!(d.used_layers(), [("Sketch1".to_owned(), 7)]);
    assert_eq!(d.on_layers(&["0".to_owned()]).entities.len(), 0);
    assert_eq!(d.on_layers(&["Sketch1".to_owned()]).entities.len(), 7);
}

#[test]
fn bounds_of_curves() {
    let d = entities(&[
        (0, "ARC"),
        (8, "A"),
        (10, "0"),
        (20, "0"),
        (40, "2"),
        (50, "0"),
        (51, "180"),
        (0, "CIRCLE"),
        (8, "B"),
        (10, "10"),
        (20, "0"),
        (40, "1"),
        (0, "LINE"),
        (8, "A"),
        (10, "0"),
        (20, "-1"),
        (11, "1"),
        (21, "-1"),
    ]);
    let (lo, hi) = d.bounds().unwrap();
    // The half circle's top (0, 2), the circle's right (11, 1) and the
    // line's y = -1.
    assert!(
        near_point(lo, p(-2.0, -1.0)) && near_point(hi, p(11.0, 2.0)),
        "{lo:?} {hi:?}"
    );
    assert_eq!(d.used_layers(), [("A".to_owned(), 2), ("B".to_owned(), 1)]);
    assert!(entities(&[]).bounds().is_none());
}
