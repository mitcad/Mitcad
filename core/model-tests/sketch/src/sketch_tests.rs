// SPDX-License-Identifier: MIT
//! Sketch commands and the sketch query, with the mock kernel.

use std::f64::consts::PI;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn doc() -> Document<MockKernel> {
    Document::new(MockKernel::default())
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn areas(doc: &Document<MockKernel>, uid: &str) -> Vec<f64> {
    let mut areas: Vec<f64> = sketch(doc, uid)["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["area"].as_f64().unwrap())
        .collect();
    areas.sort_by(f64::total_cmp);
    areas
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn assert_areas(doc: &Document<MockKernel>, uid: &str, expected: &[f64]) {
    let actual = areas(doc, uid);
    assert_eq!(actual.len(), expected.len(), "{actual:?} != {expected:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!(close(*a, *e), "{actual:?} != {expected:?}");
    }
}

/// A new document with an empty Sketch1 (F1).
fn with_sketch() -> Document<MockKernel> {
    let mut d = doc();
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn made(result: &Value) -> Vec<String> {
    result["made"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

#[test]
fn lines_with_shared_points_constraints_and_dimensions() {
    let mut d = with_sketch();
    let first = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [40, 1]}),
    );
    assert_eq!(first["entities"], json!(["p2", "p3", "c1"]));
    // Line c4 from p3 to a new p5, line c6 back to p2.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p3", "end": [20, 30]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p5", "end": "p2"}),
    );
    // A closed triangle: one region.
    assert_eq!(areas(&d, "F1").len(), 1);
    let s = sketch(&d, "F1");
    assert_eq!(s["dof"], 6);
    // Fix it down: the first point, the base horizontal and 40 long, the
    // apex 30 above the base's middle.
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": ["p2"]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": "c1"}}),
    );
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": "c1"}, "value": 40}),
    );
    assert_eq!(dim["dimensions"], json!(["k2"]));
    assert_eq!(dim["parameters"], json!(["d1"]));
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "point_line_distance", "point": "p5", "line": "c1"},
               "value": "d1 * 3 / 4"}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "equal", "a": "c4", "b": "c6"}}),
    );
    let s = sketch(&d, "F1");
    assert_eq!(s["dof"], 0, "{}", s["fully_constrained"]);
    assert_area_close(&d, 600.0);
    // A parameter change drives the sketch.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    assert_area_close(&d, 80.0 * 60.0 / 2.0);
    // An over-constraining constraint is refused, naming what blocks it in
    // words, its ids on the last line (mitcad#1566).
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "vertical", "line": "c1"}}),
    );
    let (words, blocking) = mitcad_model::sketch::conflict::split(&error);
    assert!(
        words.contains("can't hold with the new constraint") && !words.contains("k1"),
        "{error}"
    );
    assert!(blocking.is_some_and(|ids| !ids.is_empty()), "{error}");
    // A driven dimension measures; making it driving keeps the value.
    let driven = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": "c4"}, "driven": true}),
    );
    let k = driven["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F1");
    let measured = s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == k)
        .unwrap()["measured"]
        .as_f64()
        .unwrap();
    assert!(close(measured, (40f64.powi(2) + 60f64.powi(2)).sqrt()));
    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.set_driven", "sketch": "F1", "dimension": k, "driven": false})
        )
        .contains("over-constrain")
    );
}

fn assert_area_close(d: &Document<MockKernel>, expected: f64) {
    let a = areas(d, "F1");
    assert_eq!(a.len(), 1, "{a:?}");
    assert!(close(a[0], expected), "{a:?} != {expected}");
}

#[test]
fn rectangle_circle_arc_and_polygon_tools() {
    let mut d = with_sketch();
    let r = command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point",
               "a": [10, 20], "b": [0, 0]}),
    );
    assert_eq!(made(&r), ["c1", "c2", "c3", "c4"]);
    assert_eq!(r["status"]["dof"], 4);
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "three_point",
               "a": [100, 0], "b": [110, 10], "c": [100, 20]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "center",
               "center": [200, 0], "corner": [205, 3]}),
    );
    let expected_tilted = 200f64.sqrt() * (10.0 / 2f64.sqrt() + 10.0 * 2f64.sqrt()) / 2.0;
    let _ = expected_tilted;
    let a = areas(&d, "F1");
    assert_eq!(a.len(), 3);
    assert!(close(a[0], 60.0) && close(a[1], 200.0), "{a:?}");
    // Circles: by centre, two points and three points.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [0, 0], "radius": 5}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "two_point", "a": [20, 0], "b": [30, 0]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "three_point",
               "a": [45, 0], "b": [50, 5], "c": [55, 0]}),
    );
    assert_areas(&d, "F1", &[PI * 25.0, PI * 25.0, PI * 25.0]);
    // A half disk: a three-point arc closed by a line through its ends.
    let mut d = with_sketch();
    let arc = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": [10, 0], "through": [0, 10], "end": [-10, 0]}),
    );
    // c1 from p2 to p3 around p4.
    assert_eq!(arc["entities"], json!(["p2", "p3", "p4", "c1"]));
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p2", "end": "p3"}),
    );
    assert_areas(&d, "F1", &[PI * 50.0]);
    // A tangent arc continues a line.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    let tangent = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "tangent", "from": "p3", "end": [10, 10]}),
    );
    assert_eq!(tangent["constraints"], json!(["k1"]));
    let s = sketch(&d, "F1");
    let geometry = &s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "arc")
        .unwrap()["geometry"];
    assert!(
        close(geometry["radius"].as_f64().unwrap(), 5.0),
        "{geometry}"
    );
    // A hexagon on a construction circle: four degrees of freedom.
    let mut d = with_sketch();
    let hexagon = command(
        &mut d,
        json!({"cmd": "sketch.polygon", "sketch": "F1", "center": [0, 0], "vertex": [10, 0], "sides": 6}),
    );
    assert_eq!(made(&hexagon).len(), 6);
    assert_eq!(hexagon["status"]["dof"], 4);
    assert_areas(&d, "F1", &[3.0 * 3f64.sqrt() / 2.0 * 100.0]);
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.polygon", "sketch": "F1", "center": [0, 0], "vertex": [10, 0],
               "sides": 4, "inscribed": false}),
    );
    assert_areas(&d, "F1", &[400.0]);
}

#[test]
fn slots_ellipses_splines_and_text() {
    let mut d = with_sketch();
    let slot = command(
        &mut d,
        json!({"cmd": "sketch.slot", "sketch": "F1", "mode": "center_to_center",
               "a": [0, 0], "b": [30, 0], "width": 10}),
    );
    assert_eq!(made(&slot).len(), 4);
    assert_areas(&d, "F1", &[300.0 + PI * 25.0]);
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.slot", "sketch": "F1", "mode": "center_point",
               "center": [0, 0], "end": [15, 0], "width": 10}),
    );
    assert_areas(&d, "F1", &[300.0 + PI * 25.0]);
    // An arc slot: a ring sector with round ends.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.slot", "sketch": "F1", "mode": "arc",
               "center": [0, 0], "start": [50, 0], "end": [0, 50], "width": 10}),
    );
    let sector = PI / 4.0 * (55.0f64.powi(2) - 45.0f64.powi(2));
    assert_areas(&d, "F1", &[sector + PI * 25.0]);
    // An ellipse and a closed region of a spline and a line.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.ellipse", "sketch": "F1", "center": [0, 0], "major": [20, 0],
               "minor_radius": 10}),
    );
    assert_areas(&d, "F1", &[PI * 200.0]);
    let mut d = with_sketch();
    let spline = command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1", "points": [[0, 0], [10, 10], [20, 0]]}),
    );
    let pts = spline["entities"].as_array().unwrap().clone();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": pts[2], "end": pts[0]}),
    );
    assert_eq!(areas(&d, "F1").len(), 1);
    let text = command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "Mitcad", "at": [0, -20], "height": 5}),
    );
    assert_eq!(text["texts"].as_array().unwrap().len(), 1);
    assert_eq!(sketch(&d, "F1")["texts"][0]["text"], "Mitcad");
}

#[test]
fn trim_extend_and_delete() {
    // A rectangle with a vertical line through it.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point", "a": [0, 0], "b": [40, 20]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [20, -10], "end": [20, 30]}),
    ); // c9
    assert_areas(&d, "F1", &[400.0, 400.0]);
    // Trimming the ends outside leaves the line from c1 to c3.
    command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": "c9", "at": [20, -5]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": "c9", "at": [20, 25]}),
    );
    let s = sketch(&d, "F1");
    let line = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "c9")
        .unwrap()
        .clone();
    assert_eq!(
        line["geometry"],
        json!({"start": [20.0, 0.0], "end": [20.0, 20.0]})
    );
    assert_areas(&d, "F1", &[400.0, 400.0]);
    // Trimming the right half of the bottom opens the right square.
    command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": "c1", "at": [30, 0]}),
    );
    assert_areas(&d, "F1", &[400.0]);
    // Trimming the middle of the top splits it into two lines.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [30, 0]}),
    ); // c1
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [10, -5], "end": [10, 5]}),
    ); // c4
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [20, -5], "end": [20, 5]}),
    ); // c7
    let trimmed = command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": "c1", "at": [15, 0]}),
    );
    assert!(
        trimmed["constraints"].as_array().unwrap().len() >= 3,
        "{trimmed}"
    );
    let s = sketch(&d, "F1");
    let lines = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "line")
        .count();
    assert_eq!(lines, 4);
    // Extend: a line short of another reaches it.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    ); // c1
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [25, -5], "end": [25, 5]}),
    ); // c4
    command(
        &mut d,
        json!({"cmd": "sketch.extend", "sketch": "F1", "curve": "c1", "at": [9, 0]}),
    );
    let s = sketch(&d, "F1");
    let line = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "c1")
        .unwrap()
        .clone();
    assert_eq!(line["geometry"]["end"], json!([25.0, 0.0]));
    // Deleting a point takes its curve and the constraints on them.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point", "a": [0, 0], "b": [40, 20]}),
    );
    let removed = command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": ["p5"]}),
    );
    assert_eq!(removed["removed"], json!(["c1", "c4", "p5", "k1", "k4"]));
    assert!(areas(&d, "F1").is_empty());
    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.remove", "sketch": "F1", "items": ["c99"]})
        )
        .contains("does not exist")
    );
}

#[test]
fn fillet_chamfer_and_offset() {
    // A 40 x 20 rectangle of fixed size (d1, d2).
    let rectangle = |d: &mut Document<MockKernel>| {
        command(
            d,
            json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
                   "width": 40, "height": 20}),
        );
    };
    let mut d = with_sketch();
    rectangle(&mut d);
    let fillet = command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": "c1", "b": "c2", "radius": 5}),
    );
    assert_eq!(fillet["parameters"], json!(["d3"]));
    let rounded = 800.0 - 25.0 * (1.0 - PI / 4.0);
    assert_areas(&d, "F1", &[rounded]);
    assert_eq!(sketch(&d, "F1")["dof"], 0);
    let chamfer = command(
        &mut d,
        json!({"cmd": "sketch.chamfer", "sketch": "F1", "a": "c3", "b": "c4", "distance": 4}),
    );
    assert_eq!(chamfer["parameters"], json!(["d4"]));
    assert_areas(&d, "F1", &[rounded - 8.0]);
    // Both follow their parameters.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d4", "value": 6}),
    );
    assert_areas(&d, "F1", &[rounded - 18.0]);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d3", "value": 2}),
    );
    assert_areas(&d, "F1", &[800.0 - 4.0 * (1.0 - PI / 4.0) - 18.0]);
    // Offset inside a counter-clockwise rectangle: negative distance.
    let mut d = with_sketch();
    rectangle(&mut d);
    let offset = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1", "c2", "c3", "c4"],
               "distance": -5}),
    );
    assert_eq!(made(&offset).len(), 4);
    assert_eq!(offset["parameters"], json!(["d3"]));
    assert_areas(&d, "F1", &[300.0, 500.0]);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d3", "value": 2}),
    );
    assert_areas(&d, "F1", &[800.0 - 36.0 * 16.0, 36.0 * 16.0]);
    // A circle offsets out with a positive distance.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [0, 0], "radius": 10}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1"], "distance": 5}),
    );
    assert_areas(&d, "F1", &[PI * 100.0, PI * 125.0]);
}

#[test]
fn mirror_patterns_move_and_drag() {
    // A triangle mirrored about a vertical line.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, -20], "end": [0, 20],
               "construction": true}),
    ); // c1
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    ); // c4 from p5 to p6
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p6", "end": [0, 10]}),
    ); // c7 to p8
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p8", "end": "p5"}),
    ); // c9
    assert_areas(&d, "F1", &[50.0]);
    let mirrored = command(
        &mut d,
        json!({"cmd": "sketch.mirror", "sketch": "F1", "entities": ["c4", "c7", "c9"], "axis": "c1"}),
    );
    assert_eq!(made(&mirrored).len(), 3);
    assert_eq!(mirrored["constraints"].as_array().unwrap().len(), 3);
    assert_areas(&d, "F1", &[50.0, 50.0]);
    // Patterns of a circle.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [20, 0], "radius": 2}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.circular_pattern", "sketch": "F1", "entities": ["c1"],
               "center": [0, 0], "count": 4}),
    );
    assert_eq!(areas(&d, "F1").len(), 4);
    let s = sketch(&d, "F1");
    let centers: Vec<Value> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "circle")
        .map(|e| e["geometry"]["center"].clone())
        .collect();
    let top = centers.iter().any(|c| {
        (c[0].as_f64().unwrap()).abs() < 1e-9 && (c[1].as_f64().unwrap() - 20.0).abs() < 1e-9
    });
    assert!(top, "{centers:?}");
    command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": ["c1"],
               "count": [2, 3], "spacing": [10, 10]}),
    );
    assert_eq!(areas(&d, "F1").len(), 9);
    // Move and copy keep constraints; drag pulls a free point.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point", "a": [0, 0], "b": [10, 10]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": ["c1", "c2", "c3", "c4"], "by": [5, 0]}),
    );
    let s = sketch(&d, "F1");
    let p5 = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "p5")
        .unwrap()["at"]
        .clone();
    assert_eq!(p5, json!([5.0, 0.0]));
    let copy = command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": ["c1", "c2", "c3", "c4"],
               "rotate": {"center": [0, 0], "angle": PI / 2.0}, "copy": true}),
    );
    assert_eq!(made(&copy).len(), 4);
    assert_eq!(areas(&d, "F1").len(), 2);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": "p7", "to": [20, 20]}),
    );
    let s = sketch(&d, "F1");
    let p7 = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "p7")
        .unwrap()["at"]
        .clone();
    assert!(
        close(p7[0].as_f64().unwrap(), 20.0) && close(p7[1].as_f64().unwrap(), 20.0),
        "{p7}"
    );
    // Horizontal and vertical sides held: the rectangle grew.
    assert_areas(&d, "F1", &[100.0, 15.0 * 20.0]);
}

#[test]
fn dimensions_take_expressions_and_flags_change() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "w", "value": "100 mm"}),
    );
    // Binding the width to a user parameter, then an expression.
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": "k5", "value": "w"}),
    );
    assert_areas(&d, "F1", &[4000.0]);
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": "k6", "value": "w / 4"}),
    );
    assert_areas(&d, "F1", &[2500.0]);
    // d1 was used by nothing any more and went with the change.
    let params: Value =
        serde_json::from_str(&d.query(r#"{"query": "parameters"}"#).unwrap()).unwrap();
    let names: Vec<&str> = params
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["d2", "w"]);
    // Construction lines leave the profile.
    command(
        &mut d,
        json!({"cmd": "sketch.set_construction", "sketch": "F1", "curves": ["c1"]}),
    );
    assert!(areas(&d, "F1").is_empty());
    command(
        &mut d,
        json!({"cmd": "sketch.set_construction", "sketch": "F1", "curves": ["c1"], "construction": false}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.set_centerline", "sketch": "F1", "lines": ["c1"]}),
    );
    assert_eq!(areas(&d, "F1").len(), 1, "center lines bound profiles");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "diagonal"}),
    );
    assert!(error.contains("unknown rectangle mode"), "{error}");
}

#[test]
fn sketches_on_faces_and_projections_follow_the_model() {
    let mut d = doc();
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    // The top face: z = 20, facing up (the mock's end caps). Its frame is
    // the XY frame raised.
    let face = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": face}}),
    );
    // The face's edges projected, linked.
    let square = |z: f64, size: f64| -> Vec<Curve3> {
        let c = [[0.0, 0.0], [size, 0.0], [size, 40.0], [0.0, 40.0]];
        (0..4)
            .map(|i| Curve3::Line {
                start: [c[i][0], c[i][1], z],
                end: [c[(i + 1) % 4][0], c[(i + 1) % 4][1], z],
            })
            .collect()
    };
    *d.kernel().curves.borrow_mut() = square(20.0, 60.0);
    let projected = command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "linked": true}),
    );
    assert_eq!(projected["entities"].as_array().unwrap().len(), 8);
    let s = sketch(&d, "F3");
    assert_eq!(s["frame"]["origin"], json!([0.0, 0.0, 20.0]));
    assert_eq!(s["dof"], 0, "projected geometry is fixed");
    assert_eq!(s["projections"].as_array().unwrap().len(), 1);
    assert_areas(&d, "F3", &[2400.0]);
    // When the model changes, the linked projection follows.
    *d.kernel().curves.borrow_mut() = square(20.0, 80.0);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    assert_areas(&d, "F3", &[3200.0]);
    // Planes: XZ has y along -Z.
    let mut d = doc();
    command(&mut d, json!({"cmd": "sketch.create", "plane": "xz"}));
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [0, 0], "radius": 5}),
    );
    let s = sketch(&d, "F1");
    assert_eq!(s["frame"]["y_axis"], json!([0.0, 0.0, -1.0]));
    assert_eq!(s["frame"]["normal"], json!([0.0, 1.0, 0.0]));
    // YZ has the origin datum's frame.
    command(&mut d, json!({"cmd": "sketch.create", "plane": "yz"}));
    let s = sketch(&d, "F2");
    assert_eq!(s["frame"]["x_axis"], json!([0.0, 0.0, -1.0]));
    assert_eq!(s["frame"]["normal"], json!([1.0, 0.0, 0.0]));
}

/// A linked circle that an import set the start of (its seam) follows its
/// source's centre and radius and keeps where it starts (mitcad#225).
#[test]
fn a_linked_circle_keeps_where_it_starts() {
    let mut d = doc();
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let face = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
    let circle = |radius: f64| Curve3::Conic {
        center: [30.0, 20.0, 20.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: radius,
        minor: radius,
        start: 0.0,
        end: 2.0 * PI,
        closed: true,
    };
    *d.kernel().curves.borrow_mut() = vec![circle(5.0)];
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": face},
               "entities": [
                   {"id": "p1", "type": "point", "at": [30.0, 20.0], "fixed": true, "reference": true},
                   {"id": "c2", "type": "circle", "center": "p1", "radius": 5.0, "start_angle": 1.0,
                    "fixed": true, "reference": true}],
               "projections": [{"source": face, "body": "F2.b0", "entities": ["p1", "c2"]}]}}),
    );
    *d.kernel().curves.borrow_mut() = vec![circle(6.0)];
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    let s = sketch(&d, "F3");
    let c2 = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "c2")
        .unwrap();
    assert_eq!(
        (&c2["radius"], &c2["start_angle"]),
        (&json!(6.0), &json!(1.0)),
        "{s}"
    );
}

#[test]
fn a_linked_projection_with_an_arc_follows_after_other_entities() {
    let mut d = doc();
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let face = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": face}}),
    );
    // A line of the user's first: the projection's ids come after its.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [-50, 200], "end": [-45, 3]}),
    );
    // The top face with a rounded corner, as after a fillet.
    let rounded = |width: f64| -> Vec<Curve3> {
        let line = |a: [f64; 2], b: [f64; 2]| Curve3::Line {
            start: [a[0], a[1], 20.0],
            end: [b[0], b[1], 20.0],
        };
        vec![
            line([0.0, 0.0], [width - 2.0, 0.0]),
            Curve3::Conic {
                center: [width - 2.0, 2.0, 20.0],
                normal: [0.0, 0.0, 1.0],
                x_axis: [1.0, 0.0, 0.0],
                major: 2.0,
                minor: 2.0,
                start: -PI / 2.0,
                end: 0.0,
                closed: false,
            },
            line([width, 2.0], [width, 40.0]),
            line([width, 40.0], [0.0, 40.0]),
            line([0.0, 40.0], [0.0, 0.0]),
        ]
    };
    *d.kernel().curves.borrow_mut() = rounded(60.0);
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "linked": true}),
    );
    // Followed at every evaluation: the arc keeps three distinct points,
    // and the sketch evaluates (the application draws it by its frame).
    *d.kernel().curves.borrow_mut() = rounded(80.0);
    let changed = command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    assert_eq!(changed["error"], Value::Null);
    assert!(sketch(&d, "F3")["frame"].is_object());
    assert_areas(&d, "F3", &[80.0 * 40.0 - 4.0 + PI]);
}

/// The position of a point in the sketch query.
fn point_at(s: &Value, id: &str) -> [f64; 2] {
    let e = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id}"));
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

/// The id of the point at a position in the sketch query.
fn point_id(s: &Value, at: [f64; 2]) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "point"
                && close(e["at"][0].as_f64().unwrap(), at[0])
                && close(e["at"][1].as_f64().unwrap(), at[1])
        })
        .unwrap_or_else(|| panic!("no point at {at:?}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn measured(s: &Value, id: &str) -> f64 {
    s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .unwrap()["measured"]
        .as_f64()
        .unwrap()
}

#[test]
fn a_linked_projection_shows_its_followed_geometry_and_drags_its_constraints() {
    // As the application works (mitcad#40): a block, a sketch on its top
    // face with the face projected (linked), a line of the user's from a
    // projected corner and dimensions on both; then the block's sketch
    // changes. The sketch query, which the application draws and edits
    // from, shows the projection where its source is now.
    let mut d = doc();
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let face = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": face}}),
    );
    let square = |size: f64| -> Vec<Curve3> {
        let c = [[0.0, 0.0], [size, 0.0], [size, 40.0], [0.0, 40.0]];
        (0..4)
            .map(|i| Curve3::Line {
                start: [c[i][0], c[i][1], 20.0],
                end: [c[(i + 1) % 4][0], c[(i + 1) % 4][1], 20.0],
            })
            .collect()
    };
    *d.kernel().curves.borrow_mut() = square(60.0);
    // The command as the Project dialog builds it.
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "body": "F2.b0",
               "linked": true}),
    );
    let s = sketch(&d, "F3");
    let corner = point_id(&s, [60.0, 0.0]);
    let bottom = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "line"
                && e["geometry"]["start"] == json!([0.0, 0.0])
                && e["geometry"]["end"] == json!([60.0, 0.0])
        })
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // The user's line on from the corner, horizontal and 15 long.
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": corner, "end": [75, 0]}),
    );
    let user = line["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e.as_str().unwrap().starts_with('c'))
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let end = point_id(&sketch(&d, "F3"), [75.0, 0.0]);
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F3",
               "constraint": {"type": "horizontal", "line": user}}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F3",
               "dimension": {"type": "length", "line": user}, "value": 15}),
    );
    // A driven dimension on the projection measures it.
    let driven = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F3",
               "dimension": {"type": "length", "line": bottom}, "driven": true}),
    );
    let k = driven["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F3");
    assert_eq!(point_at(&s, &end), [75.0, 0.0]);
    assert!(close(measured(&s, &k), 60.0));

    // The block's width changes (a dimension of its sketch): the
    // projection, the line hanging on it and the driven dimension follow.
    *d.kernel().curves.borrow_mut() = square(80.0);
    let changed = command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    assert_eq!(changed["error"], Value::Null);
    let check = |d: &Document<MockKernel>, what: &str| {
        let s = sketch(d, "F3");
        assert_eq!(s["error"], Value::Null, "{what}");
        let at = point_at(&s, &corner);
        assert!(close(at[0], 80.0) && close(at[1], 0.0), "{what}: {at:?}");
        let at = point_at(&s, &end);
        assert!(close(at[0], 95.0) && close(at[1], 0.0), "{what}: {at:?}");
        assert!(close(measured(&s, &k), 80.0), "{what}");
        let line = s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == bottom)
            .unwrap();
        assert_eq!(line["geometry"]["end"], json!([80.0, 0.0]), "{what}");
        assert_areas(d, "F3", &[3200.0]);
    };
    check(&d, "after the change");
    // An edit of the sketch starts from the followed geometry.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [0, -20], "end": [10, -30]}),
    );
    check(&d, "after an edit");
    // And it is what the file keeps (the extrude's distance changes too).
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d3", "value": 30}),
    );
    check(&d, "after another change");
}

#[test]
fn sketches_on_construction_planes_follow_them() {
    let mut d = doc();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xz", "distance": 10}}}),
    );
    command(&mut d, json!({"cmd": "sketch.create", "plane": "F1"}));
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F2", "mode": "center", "center": [0, 0], "radius": 5}),
    );
    let s = sketch(&d, "F2");
    assert_eq!(s["plane"], "F1");
    assert_eq!(s["frame"]["origin"], json!([0.0, 10.0, 0.0]));
    assert_eq!(s["frame"]["y_axis"], json!([0.0, 0.0, -1.0]));
    // The plane moves; the sketch follows.
    command(
        &mut d,
        json!({"cmd": "edit_feature", "uid": "F1", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xz", "distance": 25}}}),
    );
    assert_eq!(sketch(&d, "F2")["frame"]["origin"], json!([0.0, 25.0, 0.0]));
    // Only a construction plane before the sketch.
    let error = rejected(&mut d, json!({"cmd": "sketch.create", "plane": "F2"}));
    assert!(error.contains("not a construction plane"), "{error}");
    let error = rejected(&mut d, json!({"cmd": "sketch.create", "plane": "F99"}));
    assert!(error.contains("F99"), "{error}");
}

#[test]
fn a_tangent_distance_is_the_gap_between_a_circle_and_a_line() {
    let mut d = with_sketch();
    // A circle of 24 at the origin (fixed) and a free line below it.
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 24}),
    );
    let circle = circle["curves"][0].as_str().unwrap().to_owned();
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [-30, -30], "end": [30, -30]}),
    );
    let line = line["entities"][2].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": line}}),
    );
    let line_y = |s: &Value| {
        let l = s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == line.as_str())
            .unwrap();
        l["geometry"]["start"][1].as_f64().unwrap()
    };
    assert!(close(line_y(&sketch(&d, "F1")), -30.0));
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_distance", "circle": circle, "line": line},
               "value": 25}),
    );
    // The gap to the nearest point of the circle: 12 + 25 below the centre.
    let s = sketch(&d, "F1");
    assert!(close(line_y(&s), -37.0), "{s}");
    assert!(close(
        measured(&s, dim["dimensions"][0].as_str().unwrap()),
        25.0
    ));
    // A driven one measures the gap.
    let driven = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_distance", "circle": circle, "line": line},
               "driven": true}),
    );
    let s = sketch(&d, "F1");
    assert!(close(
        measured(&s, driven["dimensions"][0].as_str().unwrap()),
        25.0
    ));
    // Two lines are no circle.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_distance", "circle": line, "line": line},
               "value": 3}),
    );
    assert!(!error.is_empty());
}

#[test]
fn a_concentric_distance_is_the_gap_between_two_circles() {
    // mitcad#445: a circle of 10 at the origin (fixed) and a free one of
    // radius 25 about the same centre.
    let mut d = with_sketch();
    let inner = command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10}),
    );
    // Its fixed centre: the point it added.
    let centre = inner["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| id.starts_with('p'))
        .unwrap()
        .to_owned();
    let inner = inner["curves"][0].as_str().unwrap().to_owned();
    let outer = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": centre, "radius": 25}),
    );
    let outer = outer["made"][0].as_str().unwrap().to_owned();
    let radius = |s: &Value| {
        s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == outer.as_str())
            .unwrap()["geometry"]["radius"]
            .as_f64()
            .unwrap()
    };
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "concentric_distance", "a": inner, "b": outer},
               "value": 12}),
    );
    let s = sketch(&d, "F1");
    assert!(close(radius(&s), 17.0), "{s}");
    assert!(close(
        measured(&s, dim["dimensions"][0].as_str().unwrap()),
        12.0
    ));
    // Picked the other way round, driven: the same gap.
    let driven = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "concentric_distance", "a": outer, "b": inner},
               "driven": true}),
    );
    let s = sketch(&d, "F1");
    assert!(close(
        measured(&s, driven["dimensions"][0].as_str().unwrap()),
        12.0
    ));
    // A circle with itself is refused.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "concentric_distance", "a": inner, "b": inner},
               "value": 3}),
    );
    assert!(!error.is_empty());
}

/// A trapezoid's lower left corner (mitcad#296): the fixed bottom line c1
/// from (0, 0) to (40, 0) and the side c4 from a free point at (10, 20)
/// down to the corner, 22.36 long. Their directions make 116.57 degrees,
/// the inner corner 63.43. Returns the document and the side's free point.
fn trapezoid_corner() -> (Document<MockKernel>, String) {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [40, 0]}),
    );
    let side = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [10, 20], "end": "p2"}),
    );
    assert_eq!(side["entities"], json!(["p5", "c4"]), "{side}");
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": ["c1"]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": "c4"}, "value": 500f64.sqrt()}),
    );
    (d, "p5".to_owned())
}

fn angle(reverse: Option<[bool; 2]>) -> Value {
    let mut dimension = json!({"type": "angle", "a": "c1", "b": "c4"});
    if let Some([a, b]) = reverse {
        dimension["reverse_a"] = json!(a);
        dimension["reverse_b"] = json!(b);
    }
    dimension
}

#[test]
fn an_angle_dimension_measures_on_its_side() {
    let (mut d, _) = trapezoid_corner();
    let inner = (2.0f64).atan();
    let mut ids = Vec::new();
    for reverse in [
        None,
        Some([false, false]),
        Some([false, true]),
        Some([true, false]),
        Some([true, true]),
    ] {
        let added = command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": angle(reverse), "driven": true}),
        );
        ids.push(added["dimensions"][0].as_str().unwrap().to_owned());
    }
    let s = sketch(&d, "F1");
    // Without a side (as stored before): between the directions, as before.
    let expected = [PI - inner, PI - inner, inner, inner, PI - inner];
    for (id, expected) in ids.iter().zip(expected) {
        assert!(close(measured(&s, id), expected), "{id}: {s}");
    }
    // The query gives the side as it was given.
    let dimension = |id: &str| {
        s["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == id)
            .unwrap()
            .clone()
    };
    let none = dimension(&ids[0]);
    assert!(
        none.get("reverse_a").is_none() && none.get("reverse_b").is_none(),
        "{none}"
    );
    let given = dimension(&ids[2]);
    assert_eq!(
        (&given["reverse_a"], &given["reverse_b"]),
        (&json!(false), &json!(true))
    );
}

#[test]
fn an_angle_dimension_keeps_its_side_when_solved_and_edited() {
    let length = 500f64.sqrt();
    let polar = |degrees: f64| {
        let r = degrees.to_radians();
        [length * r.cos(), length * r.sin()]
    };
    let near = |p: [f64; 2], q: [f64; 2]| {
        assert!(close(p[0], q[0]) && close(p[1], q[1]), "{p:?} != {q:?}");
    };
    // The inner corner at 100 degrees: the side opens beyond the vertical,
    // although 100 is closer to the directions' 116.57.
    let (mut d, free) = trapezoid_corner();
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": angle(Some([false, true])), "value": "100 deg"}),
    );
    near(point_at(&sketch(&d, "F1"), &free), polar(100.0));
    // A new value keeps the side, also one closer to the other angle.
    let name = added["parameters"][0].as_str().unwrap().to_owned();
    for degrees in [30.0, 150.0, 63.0] {
        command(
            &mut d,
            json!({"cmd": "set_parameter", "name": name, "value": format!("{degrees} deg")}),
        );
        near(point_at(&sketch(&d, "F1"), &free), polar(degrees));
    }
    // The other side of the side line: from the bottom line's extension.
    let (mut d, free) = trapezoid_corner();
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": angle(Some([true, true])), "value": "100 deg"}),
    );
    near(point_at(&sketch(&d, "F1"), &free), polar(80.0));
    // Without a side, as before: the angle closer to the value where the
    // lines are, here the directions' (the side at 80 degrees).
    let (mut d, free) = trapezoid_corner();
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": angle(None), "value": "100 deg"}),
    );
    near(point_at(&sketch(&d, "F1"), &free), polar(80.0));
}

// --- A partly implied constraint replaces the one it makes redundant (mitcad#287)

/// A projected line along the x axis from (0, 0) to (100, 0) (p1, p2 and
/// c3, fixed reference geometry) and a rectangle from (0, 0) to (100, 30)
/// standing on it, its left corner coincident with the line's left end.
/// With the rectangle's bottom line, its Horizontal and its two bottom
/// corners.
fn rectangle_on_projected_line() -> (Document<MockKernel>, String, String, String, String) {
    let mut d = doc();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
            {"id": "p1", "type": "point", "at": [0.0, 0.0], "fixed": true, "reference": true},
            {"id": "p2", "type": "point", "at": [100.0, 0.0], "fixed": true, "reference": true},
            {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true,
             "reference": true}]}}),
    );
    // The projected line's fixed ends hold its fixed curve, which is not
    // reported redundant on its own (mitcad#877).
    let base = sketch(&d, "F1")["redundant"].clone();
    assert_eq!(base, json!([]));
    let r = command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point",
               "a": [0, 0], "b": [100, 30]}),
    );
    let bottom = made(&r)[0].clone();
    let horizontal = r["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F1");
    let left = point_id_of(&s, [0.0, 0.0], false);
    let right = point_id_of(&s, [100.0, 0.0], false);
    assert_eq!(constraint(&s, &horizontal)["type"], "horizontal", "{s}");
    assert_eq!(constraint(&s, &horizontal)["line"], bottom.as_str(), "{s}");
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "coincident", "point": left, "entity": "p1"}}),
    );
    let s = sketch(&d, "F1");
    assert_eq!(s["redundant"], base, "{s}");
    (d, bottom, horizontal, left, right)
}

/// The id of the point at a position: a projected one or one of the
/// sketch's own.
fn point_id_of(s: &Value, at: [f64; 2], reference: bool) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "point"
                && e["reference"].as_bool().unwrap_or(false) == reference
                && close(e["at"][0].as_f64().unwrap(), at[0])
                && close(e["at"][1].as_f64().unwrap(), at[1])
        })
        .unwrap_or_else(|| panic!("no point at {at:?} in {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn constraint(s: &Value, id: &str) -> Value {
    s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["id"] == id)
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn a_partly_implied_coincident_replaces_the_constraint_it_makes_redundant() {
    let (mut d, _, horizontal, _, right) = rectangle_on_projected_line();
    let before = sketch(&d, "F1");
    let dof = before["dof"].as_u64().unwrap();
    // The right corner onto the line's right end: its height already
    // follows from the left corner and the Horizontal, its x does not.
    let result = command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "coincident", "point": right, "entity": "p2"}}),
    );
    let k = result["constraints"][0].as_str().unwrap().to_owned();
    assert_eq!(
        result["implied_removed"],
        json!([{"constraint": horizontal, "type": "horizontal"}]),
        "{result}"
    );
    assert_eq!(result["removed"], json!([horizontal]), "{result}");
    let s = sketch(&d, "F1");
    assert_eq!(constraint(&s, &horizontal), Value::Null, "{s}");
    assert_eq!(constraint(&s, &k)["type"], "coincident", "{s}");
    assert_eq!(
        s["redundant"], before["redundant"],
        "nothing more redundant: {s}"
    );
    assert_eq!(s["conflicts"], json!([]), "{s}");
    assert_eq!(s["dof"].as_u64().unwrap(), dof - 1, "{s}");
    assert!(s["error"].is_null(), "{s}");
    let at = point_at(&s, &right);
    assert!(close(at[0], 100.0) && close(at[1], 0.0), "{at:?}");
    // One undo takes both back.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d, "F1");
    assert_eq!(constraint(&s, &horizontal)["type"], "horizontal", "{s}");
    assert_eq!(constraint(&s, &k), Value::Null, "{s}");
    assert_eq!(s["dof"], before["dof"], "{s}");
    assert_eq!(s["constraints"], before["constraints"], "{s}");
}

#[test]
fn a_coincident_against_a_dimension_is_still_refused() {
    // A width that contradicts the line's length: refused, nothing removed.
    let (mut d, bottom, horizontal, _, right) = rectangle_on_projected_line();
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": bottom}, "value": 80}),
    );
    let before = sketch(&d, "F1");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "coincident", "point": right, "entity": "p2"}}),
    );
    assert!(
        error.contains("over-constrain")
            || error.contains("can't hold")
            || error.contains("cannot be solved"),
        "{error}"
    );
    let s = sketch(&d, "F1");
    assert_eq!(s["constraints"], before["constraints"], "{s}");
    assert_eq!(constraint(&s, &horizontal)["type"], "horizontal", "{s}");

    // A width that agrees with it: the coincident then follows from the
    // dimension too, which is never removed, so it is refused as well and
    // the Horizontal stays.
    let (mut d, bottom, horizontal, _, right) = rectangle_on_projected_line();
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": bottom}, "value": 100}),
    );
    let before = sketch(&d, "F1");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "coincident", "point": right, "entity": "p2"}}),
    );
    assert!(error.contains("would over-constrain"), "{error}");
    let s = sketch(&d, "F1");
    assert_eq!(s["constraints"], before["constraints"], "{s}");
    assert_eq!(constraint(&s, &horizontal)["type"], "horizontal", "{s}");
}

#[test]
fn a_constraint_the_others_imply_in_full_is_still_refused() {
    // Parallel bottom and top lines follow from their Horizontals and take
    // nothing away: no Horizontal is traded for it.
    let (mut d, bottom, _, _, _) = rectangle_on_projected_line();
    let before = sketch(&d, "F1");
    let top = before["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "line"
                && e["id"] != bottom.as_str()
                && before["constraints"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|k| k["type"] == "horizontal" && k["line"] == e["id"])
        })
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "parallel", "a": bottom, "b": top}}),
    );
    assert!(error.contains("would over-constrain"), "{error}");
    assert_eq!(
        sketch(&d, "F1")["constraints"],
        before["constraints"],
        "nothing removed"
    );
}

// --- A collapsed curve is a sketch warning (mitcad#365)

#[test]
fn a_line_collapsed_by_a_constraint_is_a_warning_about_it() {
    let mut d = doc();
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    ); // c1: p2, p3
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [20, 0], "end": [20, 10]}),
    ); // c4: p5, p6
    let s = sketch(&d, "F1");
    assert_eq!(s["degenerate"], json!([]), "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "coincident", "point": "p2", "entity": "p3"}}),
    );
    let s = sketch(&d, "F1");
    assert_eq!(s["degenerate"], json!(["c1"]), "{s}");
    let warnings = s["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{s}");
    // In plain words; what it is about in `warning_entities` (mitcad#1612).
    assert!(
        warnings[0]
            .as_str()
            .unwrap()
            .starts_with("a curve collapsed"),
        "{s}"
    );
    assert_eq!(s["warning_entities"], json!([["c1"]]), "{s}");
    // Pulling the ends apart again clears it.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d, "F1");
    assert_eq!(s["degenerate"], json!([]), "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");
}

// --- Signed horizontal and vertical distances (mitcad#404)

/// A ray from a fixed centre p1 at (5, -2) to its end p3, the end driven by
/// signed distances from the centre `r * cos(a)` and `r * sin(a)`
/// (a = 30 deg). Returns the document and the ids of the two dimensions.
fn polar_ray(signed: bool) -> (Document<MockKernel>, [String; 2]) {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [5, -2], "fixed": true}),
    );
    let ray = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p1", "end": [60, 40]}),
    );
    assert_eq!(ray["entities"], json!(["p3", "c2"]), "{ray}");
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "r", "value": "50 mm"}),
    );
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "a", "value": "30 deg"}),
    );
    let mut ids = Vec::new();
    for (kind, value) in [
        ("horizontal_distance", "r * cos(a)"),
        ("vertical_distance", "r * sin(a)"),
    ] {
        let mut dimension = json!({"type": kind, "a": "p1", "b": "p3"});
        if signed {
            dimension["signed"] = json!(true);
        }
        let added = command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1", "dimension": dimension,
                   "value": value}),
        );
        ids.push(added["dimensions"][0].as_str().unwrap().to_owned());
    }
    (d, [ids[0].clone(), ids[1].clone()])
}

#[test]
fn a_signed_point_dimension_follows_its_angle_through_all_quadrants() {
    let (mut d, [h, v]) = polar_ray(true);
    let expected = |degrees: f64| {
        let t = degrees.to_radians();
        [5.0 + 50.0 * t.cos(), -2.0 + 50.0 * t.sin()]
    };
    // Every quadrant, single steps and jumps across both axes, and back.
    for degrees in [30.0, 120.0, 210.0, 300.0, 39.0, 225.0, 135.0, 315.0, 30.0] {
        command(
            &mut d,
            json!({"cmd": "set_parameter", "name": "a", "value": format!("{degrees} deg")}),
        );
        let s = sketch(&d, "F1");
        assert!(s["error"].is_null(), "{degrees}: {s}");
        let [x, y] = point_at(&s, "p3");
        let [ex, ey] = expected(degrees);
        assert!(close(x, ex) && close(y, ey), "{degrees}: ({x}, {y})");
        assert!(close(measured(&s, &h), ex - 5.0), "{degrees}: {s}");
        assert!(close(measured(&s, &v), ey + 2.0), "{degrees}: {s}");
    }
    // The query shows the flag; a driven one measures with the sign.
    let s = sketch(&d, "F1");
    let dimension = |s: &Value, id: &str| {
        s["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(dimension(&s, &h)["signed"], true, "{s}");
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "a", "value": "210 deg"}),
    );
    let driven = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "driven": true,
               "dimension": {"type": "horizontal_distance", "a": "p1", "b": "p3",
                             "signed": true}}),
    );
    let driven = driven["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F1");
    let cos210 = 50.0 * 210f64.to_radians().cos();
    assert!(close(measured(&s, &driven), cos210), "{s}");
    // An unsigned one keeps measuring the size.
    let unsigned = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "driven": true,
               "dimension": {"type": "horizontal_distance", "a": "p1", "b": "p3"}}),
    );
    let unsigned = unsigned["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F1");
    assert!(close(measured(&s, &unsigned), -cos210), "{s}");
    assert!(dimension(&s, &unsigned).get("signed").is_none(), "{s}");
}

#[test]
fn an_unsigned_point_dimension_still_refuses_a_negative_value() {
    // As before: the sign flip of `r * cos(a)` is an error of the sketch.
    let (mut d, _) = polar_ray(false);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "a", "value": "120 deg"}),
    );
    let s = sketch(&d, "F1");
    let error = s["error"].as_str().unwrap_or_default();
    assert!(error.contains("must not be negative"), "{s}");
}

#[test]
fn signed_point_dimensions_round_trip_and_older_files_are_unchanged() {
    let (mut d, [h, v]) = polar_ray(true);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "a", "value": "250 deg"}),
    );
    let (plain, _) = polar_ray(false);
    for (doc, signed) in [(&d, true), (&plain, false)] {
        let json = doc.to_json();
        let value: Value = serde_json::from_str(&json).unwrap();
        let dimensions = value["features"][0]["dimensions"].as_array().unwrap();
        assert_eq!(dimensions.len(), 2, "{json}");
        for dimension in dimensions {
            // Only a signed dimension has the field: an unsigned one is
            // saved as before.
            assert_eq!(dimension.get("signed").is_some(), signed, "{dimension}");
        }
        let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
        assert_eq!(loaded.to_json(), json, "saving again gives the same file");
        loaded.recompute();
        assert_eq!(sketch(&loaded, "F1"), sketch(doc, "F1"));
    }
    // The loaded signed dimensions keep following their angle.
    let mut loaded = Document::from_json(&d.to_json(), MockKernel::default()).unwrap();
    command(
        &mut loaded,
        json!({"cmd": "set_parameter", "name": "a", "value": "60 deg"}),
    );
    let s = sketch(&loaded, "F1");
    assert!(s["error"].is_null(), "{s}");
    assert!(close(measured(&s, &h), 25.0), "{s}");
    assert!(
        close(measured(&s, &v), 50.0 * 60f64.to_radians().sin()),
        "{s}"
    );
}

// --- Switching a distance between signed and unsigned (mitcad#409)

/// The `dimensions` entry of the sketch query with this id.
fn dimension_entry(s: &Value, id: &str) -> Value {
    s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn set_signed(d: &mut Document<MockKernel>, id: &str, signed: bool) {
    command(
        d,
        json!({"cmd": "sketch.set_signed", "sketch": "F1", "dimension": id, "signed": signed}),
    );
}

#[test]
fn switching_a_distance_signed_keeps_its_point_where_it_is() {
    // A ray from the fixed p1 at (0, 0) to p3 at (-30, 20), left of p1.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p1", "end": [-30, 20]}),
    );
    let mut ids = Vec::new();
    for (kind, value) in [("horizontal_distance", 30), ("vertical_distance", 20)] {
        let added = command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": value,
                   "dimension": {"type": kind, "a": "p1", "b": "p3"}}),
        );
        ids.push(added["dimensions"][0].as_str().unwrap().to_owned());
    }
    let (h, v) = (ids[0].clone(), ids[1].clone());
    let at = |d: &Document<MockKernel>| point_at(&sketch(d, "F1"), "p3");
    let check = |d: &Document<MockKernel>, x: f64, hx: f64, vy: f64| {
        let s = sketch(d, "F1");
        assert!(s["error"].is_null(), "{s}");
        let [px, py] = point_at(&s, "p3");
        assert!(close(px, x) && close(py, 20.0), "({px}, {py}) {s}");
        assert!(close(measured(&s, &h), hx), "{s}");
        assert!(close(measured(&s, &v), vy), "{s}");
    };
    check(&d, -30.0, 30.0, 20.0);

    // Signed: the value becomes the offset -30, the point stays; the
    // vertical one is positive and keeps its value.
    set_signed(&mut d, &h, true);
    set_signed(&mut d, &v, true);
    check(&d, -30.0, -30.0, 20.0);
    let s = sketch(&d, "F1");
    assert_eq!(dimension_entry(&s, &h)["signed"], true, "{s}");
    assert_eq!(dimension_entry(&s, &v)["signed"], true, "{s}");
    // One undo step each: undo makes it unsigned with its size again.
    command(&mut d, json!({"cmd": "undo"}));
    command(&mut d, json!({"cmd": "undo"}));
    check(&d, -30.0, 30.0, 20.0);
    assert!(
        dimension_entry(&sketch(&d, "F1"), &h)
            .get("signed")
            .is_none()
    );
    command(&mut d, json!({"cmd": "redo"}));
    command(&mut d, json!({"cmd": "redo"}));
    check(&d, -30.0, -30.0, 20.0);
    // Switching again changes nothing.
    set_signed(&mut d, &h, true);
    check(&d, -30.0, -30.0, 20.0);

    // Unsigned again: the size, on the side where the point is.
    set_signed(&mut d, &h, false);
    check(&d, -30.0, 30.0, 20.0);
    assert!(
        dimension_entry(&sketch(&d, "F1"), &h)
            .get("signed")
            .is_none()
    );

    // An expression is negated as -(...), and switching back takes the
    // negation off instead of nesting another.
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "r", "value": "50 mm"}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": h, "value": "r * 0.6"}),
    );
    check(&d, -30.0, 30.0, 20.0);
    set_signed(&mut d, &h, true);
    check(&d, -30.0, -30.0, 20.0);
    let s = sketch(&d, "F1");
    assert_eq!(dimension_entry(&s, &h)["expression"], "-(r * 0.6)", "{s}");
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "r", "value": "100 mm"}),
    );
    check(&d, -60.0, -60.0, 20.0);
    set_signed(&mut d, &h, false);
    check(&d, -60.0, 60.0, 20.0);
    let s = sketch(&d, "F1");
    assert_eq!(dimension_entry(&s, &h)["expression"], "r * 0.6", "{s}");

    // A dimension using another parameter gets an expression of its own.
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "w", "value": "40 mm"}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": h, "value": "w"}),
    );
    check(&d, -40.0, 40.0, 20.0);
    set_signed(&mut d, &h, true);
    check(&d, -40.0, -40.0, 20.0);
    let s = sketch(&d, "F1");
    assert_eq!(dimension_entry(&s, &h)["expression"], "-w", "{s}");
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "w", "value": "10 mm"}),
    );
    assert!(close(at(&d)[0], -10.0), "{:?}", at(&d));

    // A driven one only measures the other way.
    let driven = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "driven": true,
               "dimension": {"type": "horizontal_distance", "a": "p1", "b": "p3"}}),
    );
    let driven = driven["dimensions"][0].as_str().unwrap().to_owned();
    assert!(close(measured(&sketch(&d, "F1"), &driven), 10.0));
    set_signed(&mut d, &driven, true);
    assert!(close(measured(&sketch(&d, "F1"), &driven), -10.0));
    assert!(close(at(&d)[0], -10.0), "{:?}", at(&d));

    // Only horizontal and vertical distances have a sign.
    let length = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "driven": true,
               "dimension": {"type": "length", "line": "c2"}}),
    );
    let length = length["dimensions"][0].as_str().unwrap().to_owned();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.set_signed", "sketch": "F1", "dimension": length}),
    );
    assert!(
        error.contains("not a horizontal or vertical distance"),
        "{error}"
    );
    // A negative value for a new unsigned distance is refused (the sketch
    // Dimension tool then adds it signed).
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": -5,
               "dimension": {"type": "vertical_distance", "a": "p1", "b": "p3"}}),
    );
    assert!(error.contains("must not be negative"), "{error}");
}

#[test]
fn unsigned_point_dimensions_switched_signed_follow_their_angle() {
    // Made unsigned at 30 deg, where both offsets are positive, then
    // switched: they follow the angle through all four quadrants.
    let (mut d, [h, v]) = polar_ray(false);
    set_signed(&mut d, &h, true);
    set_signed(&mut d, &v, true);
    let s = sketch(&d, "F1");
    assert_eq!(dimension_entry(&s, &h)["expression"], "r * cos(a)", "{s}");
    for degrees in [120.0, 210.0, 300.0, 30.0] {
        command(
            &mut d,
            json!({"cmd": "set_parameter", "name": "a", "value": format!("{degrees} deg")}),
        );
        let s = sketch(&d, "F1");
        assert!(s["error"].is_null(), "{degrees}: {s}");
        let t = f64::to_radians(degrees);
        let [x, y] = point_at(&s, "p3");
        assert!(
            close(x, 5.0 + 50.0 * t.cos()) && close(y, -2.0 + 50.0 * t.sin()),
            "{degrees}: ({x}, {y})"
        );
    }
}

#[test]
fn a_distance_switched_after_parameter_changes_keeps_its_point() {
    // The stored positions are those of the last edit (30 deg); at 300 deg
    // the point is below and right of the centre, and stays there through
    // both switches.
    let (mut d, [h, v]) = polar_ray(true);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "a", "value": "300 deg"}),
    );
    let t = 300f64.to_radians();
    let expected = [5.0 + 50.0 * t.cos(), -2.0 + 50.0 * t.sin()];
    let check = |d: &Document<MockKernel>, vy: f64, expression: &str| {
        let s = sketch(d, "F1");
        assert!(s["error"].is_null(), "{s}");
        let [x, y] = point_at(&s, "p3");
        assert!(
            close(x, expected[0]) && close(y, expected[1]),
            "({x}, {y}) {s}"
        );
        assert!(close(measured(&s, &h), 25.0), "{s}");
        assert!(close(measured(&s, &v), vy), "{s}");
        assert_eq!(dimension_entry(&s, &v)["expression"], expression, "{s}");
    };
    let below = 50.0 * t.sin();
    set_signed(&mut d, &v, false);
    check(&d, -below, "-(r * sin(a))");
    set_signed(&mut d, &v, true);
    check(&d, below, "r * sin(a)");
}

/// An offset of a linked projection stays on its side when the source
/// moves before the solve by more than the offset's distance (mitcad#641:
/// the solver kept each distance on the side its curves started on, so the
/// stale offset of a face edge that moved 40 mm lay on the other side of it
/// and was solved there).
#[test]
fn an_offset_of_a_projection_keeps_its_side_when_the_source_moves_far() {
    let mut d = doc();
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let face = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": face}}),
    );
    let square = |size: f64| -> Vec<Curve3> {
        let c = [[0.0, 0.0], [size, 0.0], [size, 40.0], [0.0, 40.0]];
        (0..4)
            .map(|i| Curve3::Line {
                start: [c[i][0], c[i][1], 20.0],
                end: [c[(i + 1) % 4][0], c[(i + 1) % 4][1], 20.0],
            })
            .collect()
    };
    *d.kernel().curves.borrow_mut() = square(60.0);
    let projected = command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "linked": true}),
    );
    let lines: Vec<String> = projected["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e.as_str())
        .filter(|e| e.starts_with('c'))
        .map(str::to_owned)
        .collect();
    assert_eq!(lines.len(), 4);
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": lines, "distance": 2}),
    );
    // Out by 2: the face and a band round it.
    assert_areas(&d, "F3", &[64.0 * 44.0 - 2400.0, 2400.0]);
    *d.kernel().curves.borrow_mut() = square(100.0);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 100}),
    );
    assert_areas(&d, "F3", &[104.0 * 44.0 - 4000.0, 4000.0]);
}

/// `sketch.create` with `upright` (mitcad#692, Create Sketch): the design's
/// +Z is the new sketch's y on the XZ and YZ planes and on faces facing
/// ±X and ±Y, the design's +Y on XY and on faces facing ±Z; the normal is
/// the plane's, except on the XZ origin plane, which is seen from the
/// front (-Y, mitcad#1060). The own frame is stored only where it differs
/// from the plane's, and the command stays one undo step. Without
/// `upright` the plane's frame as before.
#[test]
fn upright_sketches_put_the_design_z_up() {
    const RECT: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
    let mut d = doc();
    // A 60 x 40 x 20 block (F2.b0) whose faces face the six directions.
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": RECT}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let top = format!("F2:end({RECT})");
    let bottom = format!("F2:start({RECT})");
    let faces: [(&str, [f64; 3], [f64; 3]); 6] = [
        (&top, [30.0, 20.0, 20.0], [0.0, 0.0, 1.0]),
        (&bottom, [30.0, 20.0, 0.0], [0.0, 0.0, -1.0]),
        ("F2:side(c1[c4,c2])", [30.0, 0.0, 10.0], [0.0, -1.0, 0.0]),
        ("F2:side(c2[c1,c3])", [60.0, 20.0, 10.0], [1.0, 0.0, 0.0]),
        ("F2:side(c3[c2,c4])", [30.0, 40.0, 10.0], [0.0, 1.0, 0.0]),
        ("F2:side(c4[c3,c1])", [0.0, 20.0, 10.0], [-1.0, 0.0, 0.0]),
    ];
    for (face, origin, normal) in &faces {
        d.kernel().surfaces.borrow_mut().insert(
            (*face).to_owned(),
            mitcad_model::datum::SurfaceGeometry::Plane {
                origin: *origin,
                normal: *normal,
            },
        );
    }
    let vector =
        |value: &Value| -> [f64; 3] { std::array::from_fn(|i| value[i].as_f64().unwrap()) };
    let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9);
    let x = [1.0, 0.0, 0.0];
    let y = [0.0, 1.0, 0.0];
    let z = [0.0, 0.0, 1.0];
    let minus = |v: [f64; 3]| v.map(|c| -c);
    let on = |face: &str| json!({"face": face, "body": "F2.b0"});
    // (where, plane, the sketch's x, its y, its normal, an own frame stored)
    let cases = [
        ("XY", json!("xy"), x, y, z, false),
        ("XZ", json!("xz"), x, z, minus(y), true),
        ("YZ", json!("yz"), y, z, x, true),
        ("the top", on(&top), x, y, z, false),
        ("the bottom", on(&bottom), minus(x), y, minus(z), true),
        ("the -Y face", on(faces[2].0), x, z, minus(y), false),
        ("the +X face", on(faces[3].0), y, z, x, false),
        ("the +Y face", on(faces[4].0), minus(x), z, y, true),
        ("the -X face", on(faces[5].0), minus(y), z, minus(x), true),
    ];
    for (place, plane, want_x, want_y, want_normal, stored) in cases {
        let depth = d.undo_depth();
        let added = command(
            &mut d,
            json!({"cmd": "sketch.create", "plane": plane, "upright": true}),
        );
        let uid = added["uid"].as_str().unwrap().to_owned();
        let name = added["name"].as_str().unwrap().to_owned();
        assert_eq!(d.undo_depth(), depth + 1, "{place}: one undo step");
        assert_eq!(
            d.undo_label(),
            Some(format!("Add {name}").as_str()),
            "{place}"
        );
        let frame = sketch(&d, &uid)["frame"].clone();
        assert!(
            near(vector(&frame["x_axis"]), want_x),
            "{place}: x {}",
            frame["x_axis"]
        );
        assert!(
            near(vector(&frame["y_axis"]), want_y),
            "{place}: y {}",
            frame["y_axis"]
        );
        assert!(
            near(vector(&frame["normal"]), want_normal),
            "{place}: normal {}",
            frame["normal"]
        );
        let def: Value = serde_json::from_str(
            &d.query(&json!({"query": "feature", "uid": uid}).to_string())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            def["def"].get("frame").is_some(),
            stored,
            "{place}: {}",
            def["def"]
        );
    }
    // Without `upright` the plane's own frame: XZ's y is -Z.
    let plain = command(&mut d, json!({"cmd": "sketch.create", "plane": "xz"}));
    let frame = sketch(&d, plain["uid"].as_str().unwrap())["frame"].clone();
    assert!(
        near(vector(&frame["y_axis"]), minus(z)),
        "{}",
        frame["y_axis"]
    );
    // One undo takes an upright sketch away whole.
    command(&mut d, json!({"cmd": "undo"}));
    let depth = d.undo_depth();
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": "yz", "upright": true}),
    );
    command(&mut d, json!({"cmd": "undo"}));
    assert_eq!(
        d.undo_depth(),
        depth,
        "the upright sketch undone in one step"
    );
}
