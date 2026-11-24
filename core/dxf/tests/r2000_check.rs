// SPDX-License-Identifier: MIT
//! The written R2000 files against the structure check of the DXF
//! reference (mitcad#2315): a strict reader refused a Flat Pattern's DXF
//! that MitCad's own, lenient reader took.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use mitcad_dxf::{
    Drawing, DxfVersion, Geometry, Layer, Linetype, Point2, Spline, Text, Units, WriteOptions,
    check_r2000, read_str, write_string,
};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn layer(name: &str, color: i16) -> Layer {
    Layer {
        color,
        ..Layer::new(name)
    }
}

/// A Flat Pattern like the reported one: a plate with a slotted edge
/// whose tongues end in half circles from 270 to 450 degrees, rounded
/// corners, two holes and bend lines up and down, on the layers of a
/// process preset.
fn flat_pattern() -> Drawing {
    let mut d = Drawing::new(Units::Millimeters);
    d.layers = vec![
        layer("CUT_OUTER", 7),
        layer("CUT_INNER", 6),
        layer("BEND_UP", 3),
        layer("BEND_DOWN", 1),
    ];
    let outer = "CUT_OUTER";
    d.push(
        outer,
        Geometry::Line {
            start: p(0.0, 0.0),
            end: p(0.0, 340.0),
        },
    );
    // Rounded corners at the left edge.
    d.push(outer, Geometry::arc(p(10.0, 340.0), 10.0, FRAC_PI_2, PI));
    d.push(outer, Geometry::arc(p(10.0, 0.0), 10.0, PI, 1.5 * PI));
    let mut y = 0.0;
    for _ in 0..6 {
        // The slot's bottom, its round end and its top; then the tongue.
        d.push(
            outer,
            Geometry::Line {
                start: p(145.0, y),
                end: p(116.0, y),
            },
        );
        d.push(outer, Geometry::arc(p(116.0, y + 10.0), 10.0, PI, 1.5 * PI));
        d.push(
            outer,
            Geometry::Line {
                start: p(106.0, y + 10.0),
                end: p(106.0, y + 30.0),
            },
        );
        d.push(
            outer,
            Geometry::arc(p(116.0, y + 30.0), 10.0, FRAC_PI_2, PI),
        );
        d.push(
            outer,
            Geometry::Line {
                start: p(116.0, y + 40.0),
                end: p(145.0, y + 40.0),
            },
        );
        // The tongue's half circle crosses angle zero: 270 to 450 degrees.
        d.push(
            outer,
            Geometry::arc(p(145.0, y + 47.5), 7.5, 1.5 * PI, 2.5 * PI),
        );
        y += 55.0;
    }
    for center in [p(20.0, 20.0), p(20.0, 320.0)] {
        d.push(
            "CUT_INNER",
            Geometry::Circle {
                center,
                radius: 4.0,
            },
        );
    }
    for x in [40.0, 60.0] {
        d.push(
            "BEND_UP",
            Geometry::Line {
                start: p(x, 0.0),
                end: p(x, 340.0),
            },
        );
    }
    let mut y = 0.0;
    for _ in 0..6 {
        d.push(
            "BEND_DOWN",
            Geometry::Line {
                start: p(126.0, y + 40.0),
                end: p(126.0, y + 55.0),
            },
        );
        y += 55.0;
    }
    d
}

/// Every kind of entity and table entry the writer makes.
fn every_kind() -> Drawing {
    let mut d = Drawing::new(Units::Inches);
    let mut hidden = layer("Hidden", 5);
    hidden.visible = false;
    hidden.lineweight = Some(35);
    hidden.linetype = "PHANTOM".into();
    d.layers.push(hidden);
    d.linetypes.push(Linetype::of_dashes(
        "PHANTOM",
        "Long dash double dotted",
        &[12.0, 1.5, 0.5, 1.5, 0.5, 1.5],
    ));
    d.push("0", Geometry::Point(p(1.0, 2.0)));
    d.push(
        "Hidden",
        Geometry::Line {
            start: p(0.0, 0.0),
            end: p(3.0, 4.0),
        },
    );
    d.push(
        "Hidden",
        Geometry::Circle {
            center: p(5.0, 5.0),
            radius: 2.5,
        },
    );
    d.push("Arcs", Geometry::arc(p(-1.0, 0.0), 4.0, -0.25, 4.0));
    d.push("Arcs", Geometry::arc(p(0.0, 0.0), 1.0, 0.5, 0.5 + TAU));
    d.push(
        "Curves",
        Geometry::Ellipse {
            center: p(3.0, -2.0),
            major_axis: p(6.0, 2.0),
            ratio: 0.3,
            start_param: 0.0,
            end_param: TAU,
        },
    );
    d.push(
        "Curves",
        Geometry::Spline(Spline {
            degree: 2,
            control_points: vec![p(1.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)],
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            weights: vec![1.0, 0.5_f64.sqrt(), 1.0],
            ..Spline::default()
        }),
    );
    d.push(
        "Curves",
        Geometry::Spline(Spline {
            degree: 3,
            fit_points: vec![p(0.0, 0.0), p(2.0, 1.0), p(4.0, -1.0), p(6.0, 0.0)],
            ..Spline::default()
        }),
    );
    d.push(
        "Notes",
        Geometry::Text(Text {
            position: p(2.0, 8.0),
            height: 3.5,
            rotation: FRAC_PI_2,
            content: "Hole \u{2300}5 \u{E4}".into(),
        }),
    );
    d
}

fn r2000(drawing: &Drawing) -> String {
    write_string(drawing, &WriteOptions::default()).unwrap()
}

fn assert_passes(text: &str) {
    let problems = check_r2000(text);
    assert!(problems.is_empty(), "{problems:#?}");
}

/// `text` with `from` replaced once by `to`; `from` must be there.
fn changed(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "{from:?} is not in the text");
    text.replacen(from, to, 1)
}

/// `text` with `$HANDSEED` set to `seed`.
fn with_handseed(text: &str, seed: &str) -> String {
    let name = "$HANDSEED\n  5\n";
    let start = text.find(name).unwrap() + name.len();
    let end = start + text[start..].find('\n').unwrap();
    format!("{}{seed}{}", &text[..start], &text[end..])
}

fn assert_refused(text: &str, problem: &str) {
    let problems = check_r2000(text);
    assert!(
        problems.iter().any(|p| p.contains(problem)),
        "no problem with {problem:?} in {problems:#?}"
    );
}

#[test]
fn the_flat_pattern_passes_and_reads_back() {
    let drawing = flat_pattern();
    let text = r2000(&drawing);
    assert_passes(&text);
    let back = read_str(&text).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    assert_eq!(back.entities.len(), drawing.entities.len());
    for (a, b) in drawing.entities.iter().zip(&back.entities) {
        assert_eq!(a.layer, b.layer);
        match (&a.geometry, &b.geometry) {
            (
                Geometry::Arc {
                    center: c1,
                    radius: r1,
                    start_angle: s1,
                    end_angle: e1,
                },
                Geometry::Arc {
                    center: c2,
                    radius: r2,
                    start_angle: s2,
                    end_angle: e2,
                },
            ) => {
                assert!(c1.distance(*c2) < 1e-9 && (r1 - r2).abs() < 1e-9);
                assert!(
                    (s1 - s2).abs() < 1e-9 && (e1 - e2).abs() < 1e-9,
                    "{a:?} {b:?}"
                );
            }
            (g1, g2) => assert_eq!(g1, g2),
        }
    }
    // The tongues' ends are written from 270 to 90 degrees, not to 450.
    assert!(!text.contains("\n450"), "an angle of 450 degrees");
    // R12 has none of the R2000 structure and reads back the same.
    let r12 = write_string(
        &drawing,
        &WriteOptions {
            version: DxfVersion::R12,
            ..WriteOptions::default()
        },
    )
    .unwrap();
    let back = read_str(&r12).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    assert_eq!(back.entities.len(), drawing.entities.len());
}

#[test]
fn every_kind_the_writer_makes_passes() {
    assert_passes(&r2000(&every_kind()));
    assert_passes(&r2000(&Drawing::default()));
}

/// The R2000 frame as the writer made it before mitcad#2315, for a line
/// and an arc ending at 450 degrees: other readers took it, a strict one
/// refused it.
const BEFORE: &str = "  0
SECTION
  2
HEADER
  9
$ACADVER
  1
AC1015
  9
$DWGCODEPAGE
  3
ANSI_1252
  9
$HANDSEED
  5
2E
  9
$INSUNITS
 70
     4
  9
$MEASUREMENT
 70
     1
  0
ENDSEC
  0
SECTION
  2
CLASSES
  0
ENDSEC
  0
SECTION
  2
TABLES
  0
TABLE
  2
VPORT
  5
8
330
0
100
AcDbSymbolTable
 70
     0
  0
ENDTAB
  0
TABLE
  2
LTYPE
  5
5
330
0
100
AcDbSymbolTable
 70
     3
  0
LTYPE
  5
20
330
5
100
AcDbSymbolTableRecord
100
AcDbLinetypeTableRecord
  2
ByBlock
 70
     0
  3

 72
    65
 73
     0
 40
0.0
  0
LTYPE
  5
21
330
5
100
AcDbSymbolTableRecord
100
AcDbLinetypeTableRecord
  2
ByLayer
 70
     0
  3

 72
    65
 73
     0
 40
0.0
  0
LTYPE
  5
22
330
5
100
AcDbSymbolTableRecord
100
AcDbLinetypeTableRecord
  2
Continuous
 70
     0
  3
Solid line
 72
    65
 73
     0
 40
0.0
  0
ENDTAB
  0
TABLE
  2
LAYER
  5
2
330
0
100
AcDbSymbolTable
 70
     2
  0
LAYER
  5
23
330
2
100
AcDbSymbolTableRecord
100
AcDbLayerTableRecord
  2
0
 70
     0
 62
     7
  6
Continuous
370
    -3
  0
LAYER
  5
24
330
2
100
AcDbSymbolTableRecord
100
AcDbLayerTableRecord
  2
CUT_OUTER
 70
     0
 62
     7
  6
Continuous
370
    -3
  0
ENDTAB
  0
TABLE
  2
STYLE
  5
3
330
0
100
AcDbSymbolTable
 70
     1
  0
STYLE
  5
25
330
3
100
AcDbSymbolTableRecord
100
AcDbTextStyleTableRecord
  2
Standard
 70
     0
 40
0.0
 41
1.0
 50
0.0
 71
     0
 42
2.5
  3
txt
  4

  0
ENDTAB
  0
TABLE
  2
VIEW
  5
6
330
0
100
AcDbSymbolTable
 70
     0
  0
ENDTAB
  0
TABLE
  2
UCS
  5
7
330
0
100
AcDbSymbolTable
 70
     0
  0
ENDTAB
  0
TABLE
  2
APPID
  5
9
330
0
100
AcDbSymbolTable
 70
     1
  0
APPID
  5
26
330
9
100
AcDbSymbolTableRecord
100
AcDbRegAppTableRecord
  2
ACAD
 70
     0
  0
ENDTAB
  0
TABLE
  2
DIMSTYLE
  5
A
330
0
100
AcDbSymbolTable
 70
     1
100
AcDbDimStyleTable
  0
DIMSTYLE
105
27
330
A
100
AcDbSymbolTableRecord
100
AcDbDimStyleTableRecord
  2
Standard
 70
     0
  0
ENDTAB
  0
TABLE
  2
BLOCK_RECORD
  5
1
330
0
100
AcDbSymbolTable
 70
     2
  0
BLOCK_RECORD
  5
11
330
1
100
AcDbSymbolTableRecord
100
AcDbBlockTableRecord
  2
*Model_Space
  0
BLOCK_RECORD
  5
12
330
1
100
AcDbSymbolTableRecord
100
AcDbBlockTableRecord
  2
*Paper_Space
  0
ENDTAB
  0
ENDSEC
  0
SECTION
  2
BLOCKS
  0
BLOCK
  5
28
330
11
100
AcDbEntity
  8
0
100
AcDbBlockBegin
  2
*Model_Space
 70
     0
 10
0.0
 20
0.0
 30
0.0
  3
*Model_Space
  1

  0
ENDBLK
  5
29
330
11
100
AcDbEntity
  8
0
100
AcDbBlockEnd
  0
BLOCK
  5
2A
330
12
100
AcDbEntity
 67
     1
  8
0
100
AcDbBlockBegin
  2
*Paper_Space
 70
     0
 10
0.0
 20
0.0
 30
0.0
  3
*Paper_Space
  1

  0
ENDBLK
  5
2B
330
12
100
AcDbEntity
 67
     1
  8
0
100
AcDbBlockEnd
  0
ENDSEC
  0
SECTION
  2
ENTITIES
  0
LINE
  5
2C
330
11
100
AcDbEntity
  8
CUT_OUTER
100
AcDbLine
 10
0.0
 20
0.0
 30
0.0
 11
100.0
 21
0.0
 31
0.0
  0
ARC
  5
2D
330
11
100
AcDbEntity
  8
CUT_OUTER
100
AcDbCircle
 10
100.0
 20
7.5
 30
0.0
 40
7.5
100
AcDbArc
 50
270.0
 51
450.0
  0
ENDSEC
  0
SECTION
  2
OBJECTS
  0
DICTIONARY
  5
C
330
0
100
AcDbDictionary
281
     1
  3
ACAD_GROUP
350
D
  0
DICTIONARY
  5
D
330
C
100
AcDbDictionary
281
     1
  0
ENDSEC
  0
EOF
";

#[test]
fn the_frame_written_before_is_refused_and_the_one_written_now_passes() {
    // MitCad's reader takes the old frame.
    let back = read_str(BEFORE).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    assert_eq!(back.entities.len(), 2);
    // The check names each fault: the dimension style table's group 71,
    // the layers' plot style, the dimension style's text style, the arc's
    // end angle.
    let problems = check_r2000(BEFORE);
    for problem in [
        "TABLE: group 71 is missing",
        "LAYER: group 390 is missing",
        "DIMSTYLE: group 340 is missing",
        "ARC: group 51 is not an angle in [0, 360)",
    ] {
        assert!(
            problems.iter().any(|p| p.contains(problem)),
            "no {problem:?} in {problems:#?}"
        );
    }
    // The same drawing written now passes.
    let mut drawing = Drawing::new(Units::Millimeters);
    drawing.layers.push(layer("CUT_OUTER", 7));
    drawing.push(
        "CUT_OUTER",
        Geometry::Line {
            start: p(0.0, 0.0),
            end: p(100.0, 0.0),
        },
    );
    drawing.push(
        "CUT_OUTER",
        Geometry::arc(p(100.0, 7.5), 7.5, 1.5 * PI, 2.5 * PI),
    );
    assert_passes(&r2000(&drawing));
}

#[test]
fn the_check_is_strict() {
    let text = r2000(&flat_pattern());
    assert_passes(&text);
    // Each fault the strict reader could stop at is found.
    let cases = [
        (
            changed(
                &text,
                "AcDbDimStyleTable\n 71\n     0\n",
                "AcDbDimStyleTable\n",
            ),
            "group 71 is missing",
        ),
        (
            changed(&text, "370\n    -3\n390\nF\n", "370\n    -3\n"),
            "group 390 is missing",
        ),
        // Two groups swapped.
        (
            changed(&text, " 40\n0.0\n 41\n1.0\n", " 41\n1.0\n 40\n0.0\n"),
            "STYLE: expected group 40, found group 41",
        ),
        // A pointer to nothing and one to the wrong kind.
        (
            changed(&text, "390\nF\n", "390\nFFFFF\n"),
            "which no record has",
        ),
        (
            changed(&text, "390\nF\n", "390\nE\n"),
            "names a ACDBDICTIONARYWDFLT, not a ACDBPLACEHOLDER",
        ),
        // A class missing, a handle over the seed, a value of the wrong type.
        (
            changed(
                &text,
                "  0\nCLASS\n  1\nACDBPLACEHOLDER\n",
                "  0\nCLASS\n  1\nACDBPLACEHOLDERX\n",
            ),
            "ACDBPLACEHOLDER has no CLASS",
        ),
        (with_handseed(&text, "1"), "is not below $HANDSEED"),
        (
            changed(&text, " 62\n     7\n", " 62\n   7.5\n"),
            "group 62 needs an integer",
        ),
        // A table out of its order.
        (
            changed(&text, "  2\nVIEW\n", "  2\nUCS\n"),
            "expected table VIEW",
        ),
    ];
    for (text, problem) in &cases {
        assert_refused(text, problem);
    }
    // An R12 file is no R2000 file.
    let r12 = write_string(
        &flat_pattern(),
        &WriteOptions {
            version: DxfVersion::R12,
            ..WriteOptions::default()
        },
    )
    .unwrap();
    assert_refused(&r12, "is not R2000");
}
