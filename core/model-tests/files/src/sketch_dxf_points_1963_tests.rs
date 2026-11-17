// SPDX-License-Identifier: MIT
//! A sketch's points in its DXF (mitcad#1963, GitHub mitcad/Mitcad#408),
//! with the mock kernel:
//!
//! - `export_sketch` writes the sketch's free-standing points as POINT
//!   entities on the layer `POINTS`, not the points its curves refer to,
//!   nor the sketch's origin point and its axes' ends; in the file's unit;
//!   `"points": false` leaves them out, and the result says which was done.
//! - The reporter's design (`MITCAD_MITCAD_REPORTED`/`github-408`, when it
//!   is there): its four points as POINT entities on `POINTS` where the
//!   sketch has them, none with `points: false`, and back as sketch points
//!   through Insert DXF.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_dxf::{Geometry, Point2};
use mitcad_model::Document;
use mitcad_model::test_support::block;
use mitcad_model::testing::{MockKernel, remove_temp, unique_temp_dir};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() <= 1e-6 && (a[1] - b[1]).abs() <= 1e-6
}

/// The POINT entities of a DXF file: their layers and places.
fn points_of(path: &str) -> Vec<(String, [f64; 2])> {
    mitcad_dxf::read_file(path)
        .unwrap()
        .entities
        .iter()
        .filter_map(|e| match e.geometry {
            Geometry::Point(Point2 { x, y }) => Some((e.layer.clone(), [x, y])),
            _ => None,
        })
        .collect()
}

/// Whether `found` holds the places of `expected`, each once, on `layer`.
fn same_points(found: &[(String, [f64; 2])], layer: &str, expected: &[[f64; 2]]) -> bool {
    found.len() == expected.len()
        && found.iter().all(|(l, _)| l == layer)
        && expected
            .iter()
            .all(|at| found.iter().filter(|(_, p)| near(*p, *at)).count() == 1)
}

#[test]
fn a_sketch_s_free_points_are_written_as_points_unless_left_out() {
    let mut b = block();
    // Two free points (hole marks), and the sketch's x axis, which makes
    // its origin point and the axis's far end: neither is written.
    for at in [[15.0, 10.0], [45.0, 30.0]] {
        command(
            &mut b.doc,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": at}),
        );
    }
    command(
        &mut b.doc,
        json!({"cmd": "sketch.axis", "sketch": "F1", "axis": "x"}),
    );
    let dir = unique_temp_dir("1963-sketch-points");
    let path = dir.join("sketch.dxf").to_str().unwrap().to_owned();

    // By default: the rectangle's four lines and the two points.
    let result = command(
        &mut b.doc,
        json!({"cmd": "export_sketch", "sketch": "Sketch1", "path": path}),
    );
    assert_eq!(result["entities"], 6, "{result}");
    assert_eq!(result["points"], true, "{result}");
    let found = points_of(&path);
    assert!(
        same_points(&found, "POINTS", &[[15.0, 10.0], [45.0, 30.0]]),
        "{found:?}"
    );
    // The lines stay on the layer 0, the points do not join them.
    let drawing = mitcad_dxf::read_file(&path).unwrap();
    assert_eq!(
        drawing
            .entities
            .iter()
            .filter(|e| e.layer == "0" && matches!(e.geometry, Geometry::Line { .. }))
            .count(),
        4
    );

    // Construction geometry does not change the points written (the axis's
    // line is never written).
    let result = command(
        &mut b.doc,
        json!({"cmd": "export_sketch", "sketch": "Sketch1", "path": path,
               "construction": true}),
    );
    assert_eq!(result["entities"], 6, "{result}");
    assert_eq!(points_of(&path).len(), 2);

    // In inches, each point where the sketch has it in that unit.
    let result = command(
        &mut b.doc,
        json!({"cmd": "export_sketch", "sketch": "Sketch1", "path": path, "units": "in"}),
    );
    assert_eq!(result["units"], "in", "{result}");
    let found = points_of(&path);
    assert!(
        same_points(
            &found,
            "POINTS",
            &[[15.0 / 25.4, 10.0 / 25.4], [45.0 / 25.4, 30.0 / 25.4]]
        ),
        "{found:?}"
    );

    // Left out.
    let result = command(
        &mut b.doc,
        json!({"cmd": "export_sketch", "sketch": "Sketch1", "path": path, "points": false}),
    );
    assert_eq!(result["entities"], 4, "{result}");
    assert_eq!(result["points"], false, "{result}");
    assert!(points_of(&path).is_empty());

    // Only true or false.
    assert!(
        b.doc
            .command(
                &json!({"cmd": "export_sketch", "sketch": "Sketch1", "path": path, "points": "no"})
                    .to_string(),
            )
            .is_err()
    );
    remove_temp(&dir);
}

/// The reporter's design of GitHub mitcad/Mitcad#408 unpacked from its zip
/// into `dir` and loaded with the mock kernel; None, and a note, when the
/// private test files are not here.
fn reported(dir: &Path) -> Option<Document<MockKernel>> {
    let root = std::env::var("MITCAD_MITCAD_REPORTED").ok()?;
    let zip = PathBuf::from(root)
        .join("github-408")
        .join("825x625.mitcad.zip");
    if !zip.is_file() {
        eprintln!("skipped: {} is not here", zip.display());
        return None;
    }
    let archive = mitcad_zip::Archive::open(&zip).unwrap();
    let entry = archive
        .entries()
        .iter()
        .find(|e| e.name == "825x625.mitcad")
        .unwrap_or_else(|| panic!("no design in {}", zip.display()));
    let design = dir.join("825x625.mitcad");
    std::fs::write(&design, archive.read(entry).unwrap()).unwrap();
    let mut doc = Document::load_design(&design, MockKernel::default())
        .unwrap_or_else(|e| panic!("{}: {e}", design.display()));
    doc.recompute();
    Some(doc)
}

#[test]
fn the_reported_design_s_points_are_written_and_read_back() {
    let dir = unique_temp_dir("1963-reported");
    let Some(mut doc) = reported(&dir) else {
        remove_temp(&dir);
        return;
    };
    // The hole marks the reporter placed: three along the bar's middle and
    // one above it.
    let marks = [
        [159.04002066399306, 12.7],
        [369.1700619919792, 12.7],
        [528.5001033199652, 12.7],
        [550.0, 120.0],
    ];
    let path = dir.join("bar.dxf").to_str().unwrap().to_owned();
    let result = command(
        &mut doc,
        json!({"cmd": "export_sketch", "sketch": "Sketch1", "path": path}),
    );
    assert_eq!(result["units"], "mm", "{result}");
    // The outline's four lines and the four points.
    assert_eq!(result["entities"], 8, "{result}");
    let found = points_of(&path);
    assert!(same_points(&found, "POINTS", &marks), "{found:?}");

    // Read back by Insert DXF: the points are sketch points again.
    let mut back = Document::new(MockKernel::default());
    command(&mut back, json!({"cmd": "sketch.create"}));
    command(
        &mut back,
        json!({"cmd": "sketch.import_dxf", "sketch": "F1", "path": path}),
    );
    let sketch: Value = serde_json::from_str(
        &back
            .query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap();
    let points: Vec<[f64; 2]> = sketch["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "point")
        .map(|e| [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()])
        .collect();
    for mark in marks {
        assert!(
            points.iter().any(|p| near(*p, mark)),
            "{mark:?} not read back: {points:?}"
        );
    }

    // Without points: the outline alone.
    let result = command(
        &mut doc,
        json!({"cmd": "export_sketch", "sketch": "Sketch1", "path": path, "points": false}),
    );
    assert_eq!(result["entities"], 4, "{result}");
    assert!(points_of(&path).is_empty());
    remove_temp(&dir);
}
