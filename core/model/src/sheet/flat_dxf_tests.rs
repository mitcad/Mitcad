// SPDX-License-Identifier: MIT
//! The Flat Pattern drawing (mitcad#210), round trips through `mitcad-dxf`.

use super::*;
use crate::ids::FeatureUid;
use crate::sheet::flat::FlatLine;
use crate::sheet::{
    BendId, BoundedCurve2, CurvePieceRef, MaterialEdge, MaterialLoop, SheetCurveId,
};
use mitcad_dxf::{Geometry, Units};

fn uid(n: u64) -> FeatureUid {
    format!("F{n}").parse().unwrap()
}

fn edge(slot: u32, geometry: SegmentGeometry) -> MaterialEdge {
    MaterialEdge {
        piece: CurvePieceRef {
            curve: SheetCurveId::Generated {
                feature: uid(3),
                slot,
            },
            piece: 0,
        },
        curve: BoundedCurve2::from_geometry(geometry).unwrap(),
    }
}

fn line(slot: u32, start: [f64; 2], end: [f64; 2]) -> MaterialEdge {
    edge(slot, SegmentGeometry::Line { start, end })
}

/// The developed L of the contract (`BA = 1.8 pi`): a counter-clockwise
/// outline split at both tangent seams, with a radius-2 hole made of two
/// half circles traversed clockwise, and one bend.
fn developed(direction: &'static str) -> (Vec<MaterialRegion>, Vec<FlatBend>) {
    let ba = 1.8 * std::f64::consts::PI;
    let end = 15.0 + ba;
    let outer = MaterialLoop {
        edges: vec![
            line(0, [-30.0, 0.0], [0.0, 0.0]),
            line(1, [0.0, 0.0], [ba, 0.0]),
            line(2, [ba, 0.0], [end, 0.0]),
            line(3, [end, 0.0], [end, 20.0]),
            line(4, [end, 20.0], [ba, 20.0]),
            line(5, [ba, 20.0], [0.0, 20.0]),
            line(6, [0.0, 20.0], [-30.0, 20.0]),
            line(7, [-30.0, 20.0], [-30.0, 0.0]),
        ],
    };
    let mut first = edge(
        8,
        SegmentGeometry::Arc {
            center: [-15.0, 10.0],
            radius: 2.0,
            start_angle: 0.0,
            end_angle: std::f64::consts::PI,
        },
    );
    first.curve.range = first.curve.range.reversed();
    let mut second = edge(
        9,
        SegmentGeometry::Arc {
            center: [-15.0, 10.0],
            radius: 2.0,
            start_angle: std::f64::consts::PI,
            end_angle: std::f64::consts::TAU,
        },
    );
    second.curve.range = second.curve.range.reversed();
    let hole = MaterialLoop {
        edges: vec![second, first],
    };
    let bend = FlatBend {
        bend: BendId {
            feature: uid(4),
            slot: 0,
        },
        angle: std::f64::consts::FRAC_PI_2,
        radius: 3.0,
        k_factor: 0.3,
        allowance: ba,
        direction,
        centerlines: vec![FlatLine {
            start: [ba / 2.0, 20.0],
            end: [ba / 2.0, 0.0],
        }],
        slits: Vec::new(),
        tangents: vec![
            FlatLine {
                start: [0.0, 20.0],
                end: [0.0, 0.0],
            },
            FlatLine {
                start: [ba, 20.0],
                end: [ba, 0.0],
            },
        ],
    };
    (
        vec![MaterialRegion {
            loops: vec![outer, hole],
        }],
        vec![bend],
    )
}

fn round_trip(drawing: &mitcad_dxf::Drawing) -> mitcad_dxf::Drawing {
    let text = mitcad_dxf::write_string(drawing, &mitcad_dxf::WriteOptions::default()).unwrap();
    mitcad_dxf::read_str(&text).unwrap()
}

fn on<'a>(drawing: &'a mitcad_dxf::Drawing, layer: &str) -> Vec<&'a Geometry> {
    drawing
        .entities
        .iter()
        .filter(|e| e.layer == layer)
        .map(|e| &e.geometry)
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn seams_are_joined_and_each_contour_is_written_once() {
    let (regions, bends) = developed("down");
    let back = round_trip(&drawing(&regions, &bends, &FlatDxfOptions::default()).unwrap());
    assert_eq!(back.units, Units::Millimeters);
    // The four sides of the outline, each one line across both seams.
    let outer = on(&back, CUT_OUTER);
    assert_eq!(outer.len(), 4, "{outer:?}");
    let end = 15.0 + 1.8 * std::f64::consts::PI;
    assert!(
        outer
            .iter()
            .any(|g| matches!(g, Geometry::Line { start, end: e }
        if close(start.x.min(e.x), -30.0) && close(start.x.max(e.x), end)
            && close(start.y, 0.0) && close(e.y, 0.0)))
    );
    // The hole's two halves are one circle.
    let inner = on(&back, CUT_INNER);
    assert_eq!(inner.len(), 1);
    assert!(matches!(inner[0], Geometry::Circle { center, radius, .. }
        if close(center.x, -15.0) && close(center.y, 10.0) && close(*radius, 2.0)));
    // The bend's centre line, across the whole width at u = BA/2.
    let down = on(&back, BEND_DOWN);
    assert_eq!(down.len(), 1);
    assert!(matches!(down[0], Geometry::Line { start, end }
        if close(start.x, 0.9 * std::f64::consts::PI) && close(end.x, start.x)
            && close((start.y - end.y).abs(), 20.0)));
    assert!(on(&back, BEND_UP).is_empty());
    // No tangent lines unless asked for; the layers are declared anyway.
    assert!(on(&back, BEND_TANGENT).is_empty());
    for layer in [CUT_OUTER, CUT_INNER, BEND_UP, BEND_DOWN] {
        assert!(back.layer(layer).is_some(), "{layer}");
    }
    assert_eq!(back.entities.len(), 6);
}

#[test]
fn direction_layers_tangents_and_units() {
    let (regions, bends) = developed("up");
    let options = FlatDxfOptions {
        unit: LengthUnit::Inch,
        tangents: true,
        ..FlatDxfOptions::default()
    };
    let back = round_trip(&drawing(&regions, &bends, &options).unwrap());
    assert_eq!(back.units, Units::Inches);
    assert_eq!(on(&back, BEND_UP).len(), 1);
    assert!(on(&back, BEND_DOWN).is_empty());
    assert_eq!(on(&back, BEND_TANGENT).len(), 2);
    // Coordinates in inches: the outline is 30 + 15 + 1.8 pi mm long.
    let (min, max) = back.on_layers(&[CUT_OUTER.to_owned()]).bounds().unwrap();
    let length = (45.0 + 1.8 * std::f64::consts::PI) / 25.4;
    assert!(close(max.x - min.x, length), "{min:?} {max:?}");
    assert!(close(max.y - min.y, 20.0 / 25.4));
    // In millimetres again, through the unit the file names.
    let mm = back.to_millimeters(Units::Unitless);
    let (min, _) = mm.bounds().unwrap();
    assert!(close(min.x, -30.0) && close(min.y, 0.0), "{min:?}");
}

#[test]
fn only_pieces_of_one_line_or_circle_are_joined() {
    // An L-shaped outline: the corner lines stay apart, and so do a line
    // and an arc that meet.
    let pieces = vec![
        SegmentGeometry::Line {
            start: [0.0, 0.0],
            end: [10.0, 0.0],
        },
        SegmentGeometry::Line {
            start: [10.0, 0.0],
            end: [10.0, 5.0],
        },
        SegmentGeometry::Arc {
            center: [5.0, 5.0],
            radius: 5.0,
            start_angle: 0.0,
            end_angle: std::f64::consts::PI,
        },
        SegmentGeometry::Line {
            start: [0.0, 5.0],
            end: [0.0, 2.0],
        },
        SegmentGeometry::Line {
            start: [0.0, 2.0],
            end: [0.0, 0.0],
        },
    ];
    let joined = joined(pieces);
    assert_eq!(joined.len(), 4, "{joined:?}");
    assert!(joined.contains(&SegmentGeometry::Line {
        start: [0.0, 5.0],
        end: [0.0, 0.0]
    }));
    // A line folding back on itself is not one line.
    let folded = joined_pair(
        SegmentGeometry::Line {
            start: [0.0, 0.0],
            end: [10.0, 0.0],
        },
        SegmentGeometry::Line {
            start: [10.0, 0.0],
            end: [4.0, 0.0],
        },
    );
    assert_eq!(folded, 2);
}

fn joined_pair(a: SegmentGeometry, b: SegmentGeometry) -> usize {
    join(&a, &b).map_or(2, |_| 1)
}

/// A board's die line (mitcad#1939): its contours on one cut layer and
/// its folds on the crease layers, a reverse crease on its own; a kerf
/// bend's slits on the inner cut layer in place of its centre line.
#[test]
fn process_profiles_name_the_layers() {
    let board = FlatDxfOptions {
        process: SheetProcess::Board,
        ..FlatDxfOptions::default()
    };
    let (regions, bends) = developed("up");
    let back = round_trip(&drawing(&regions, &bends, &board).unwrap());
    // (The reader adds DXF's own layer 0.)
    let names: Vec<&str> = back
        .layers
        .iter()
        .map(|l| l.name.as_str())
        .filter(|n| *n != "0")
        .collect();
    assert_eq!(names, ["CUT", "CREASE", "CREASE_REVERSE"]);
    assert!(on(&back, CUT_OUTER).is_empty());
    assert!(!on(&back, "CUT").is_empty());
    assert_eq!(on(&back, "CREASE").len(), 1);
    let (regions, bends) = developed("down");
    let back = round_trip(&drawing(&regions, &bends, &board).unwrap());
    assert_eq!(on(&back, "CREASE_REVERSE").len(), 1);

    let laser = FlatDxfOptions {
        process: SheetProcess::LaserSheet,
        ..FlatDxfOptions::default()
    };
    let (regions, mut bends) = developed("up");
    let inner = on(
        &round_trip(&drawing(&regions, &bends, &laser).unwrap()),
        CUT_INNER,
    )
    .len();
    bends[0].slits = vec![
        FlatLine {
            start: [1.0, 2.0],
            end: [1.0, 8.0],
        },
        FlatLine {
            start: [2.0, 12.0],
            end: [2.0, 18.0],
        },
    ];
    let back = round_trip(&drawing(&regions, &bends, &laser).unwrap());
    assert_eq!(on(&back, CUT_INNER).len(), inner + 2);
    assert!(on(&back, "SCORE").is_empty());
}

#[test]
fn every_process_writes_a_file_strict_readers_take() {
    // mitcad#2315: a strict reader refused a Flat Pattern's R2000 file that
    // MitCad's own reader took; the structure check holds the written text
    // to the DXF reference, for each process, direction and unit.
    for process in SheetProcess::ALL {
        for direction in ["up", "down"] {
            for unit in [LengthUnit::Millimetre, LengthUnit::Inch] {
                let (regions, bends) = developed(direction);
                let options = FlatDxfOptions {
                    unit,
                    tangents: true,
                    process,
                };
                let flat = drawing(&regions, &bends, &options).unwrap();
                let text =
                    mitcad_dxf::write_string(&flat, &mitcad_dxf::WriteOptions::default()).unwrap();
                let problems = mitcad_dxf::check_r2000(&text);
                assert!(
                    problems.is_empty(),
                    "{process:?} {direction}: {problems:#?}"
                );
            }
        }
    }
}
