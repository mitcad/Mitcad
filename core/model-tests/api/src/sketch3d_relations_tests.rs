// SPDX-License-Identifier: MIT
//! 3D relations and dimensions of mitcad#1661 through the JSON api, with the
//! mock kernel: Symmetric about a construction plane and On Plane of
//! another sketch's plane (found where they are, followed when they move),
//! Parallel, Perpendicular, an Angle, Concentric and Curvature saved and
//! read again; a driven dimension's reference parameter used by a driving
//! dimension of the same sketch (solved in stages, following its source),
//! and one that would move what it uses refused as a circular dependency.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const TOL: f64 = 1e-6;

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
    let query = json!({"query": "sketch3d", "uid": uid});
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

fn xyz(value: &Value) -> [f64; 3] {
    let a = value
        .as_array()
        .unwrap_or_else(|| panic!("{value} is not a point"));
    [0, 1, 2].map(|k| a[k].as_f64().unwrap())
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= TOL)
}

fn at(sketch: &Value, id: &str) -> [f64; 3] {
    let point = sketch["points"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap_or_else(|| panic!("{id} in {sketch}"));
    xyz(&point["at"])
}

fn item<'a>(sketch: &'a Value, list: &str, id: &Value) -> &'a Value {
    sketch[list]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == *id)
        .unwrap_or_else(|| panic!("{id} in {list} of {sketch}"))
}

fn made(result: &Value) -> Value {
    result["made"][0].clone()
}

#[test]
fn relations_about_planes_outside_the_sketch_are_saved_and_read_again() {
    let mut doc = Document::new(MockKernel::default());
    // F1: a construction plane 10 above XY; F2: a planar sketch on it.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 10}}}),
    );
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "F1"}));
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let s = "F3";
    // Symmetric about the construction plane: p2 mirrors p1 across z = 10.
    let p1 = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": s, "at": [5, 3, 2], "fixed": true}),
    );
    let p2 = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": s, "at": [6, 2, 30]}),
    );
    let (a, b) = (made(&p1), made(&p2));
    let symmetric = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": s, "constraint":
               {"type": "symmetric", "a": a, "b": b, "plane": {"construction": "F1"}}}),
    );
    let view = sketch(&doc, s);
    assert!(
        near(at(&view, b.as_str().unwrap()), [5.0, 3.0, 18.0]),
        "{view}"
    );
    // On Plane of the planar sketch's plane: a free point onto z = 10.
    let p3 = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": s, "at": [20, 20, 3]}),
    );
    let c = made(&p3);
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": s, "constraint":
               {"type": "on_plane", "points": [c], "plane": {"sketch": "F2"}}}),
    );
    let view = sketch(&doc, s);
    assert!(
        (at(&view, c.as_str().unwrap())[2] - 10.0).abs() <= TOL,
        "{view}"
    );
    // Lines: parallel, perpendicular, at an angle.
    let l1 = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": s, "start": [0, -10, 0],
               "end": [40, -10, 0], "axis": "x", "length": 40}),
    );
    let l2 = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": s, "start": [0, -30, 5],
               "end": [30, -35, 15]}),
    );
    let l3 = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": s, "start": [60, 0, 0],
               "end": [62, 30, 8]}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": s, "constraint":
               {"type": "parallel", "a": made(&l1), "b": made(&l2)}}),
    );
    let angle = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": s,
               "dimension": {"type": "angle", "a": made(&l1), "b": made(&l3)}, "value": "60 deg"}),
    );
    let view = sketch(&doc, s);
    let measured = item(&view, "dimensions", &made(&angle))["measured"]
        .as_f64()
        .unwrap();
    assert!((measured - 60f64.to_radians()).abs() <= 1e-9, "{measured}");
    // A fourth line square to the third (the second is parallel to the
    // first, at 60° to the third: square to it would contradict them).
    let l4 = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": s, "start": [80, 0, 0],
               "end": [85, 10, 20]}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": s, "constraint":
               {"type": "perpendicular", "a": made(&l3), "b": made(&l4)}}),
    );
    let view = sketch(&doc, s);
    assert_eq!(view["status"]["redundant"], json!([]), "{view}");

    // The construction plane moves: the mirrored point follows it.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F1", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 12}}}),
    );
    let view = sketch(&doc, s);
    assert!(
        near(at(&view, b.as_str().unwrap()), [5.0, 3.0, 22.0]),
        "{view}"
    );
    assert!(
        (at(&view, c.as_str().unwrap())[2] - 12.0).abs() <= TOL,
        "{view}"
    );

    // Saved and read again: the same relations and geometry.
    let file = doc.to_json();
    let saved: Value = serde_json::from_str(&file).unwrap();
    let mut back = Document::from_json(&file, MockKernel::default()).unwrap();
    back.recompute();
    let read = sketch(&back, s);
    assert_eq!(read["constraints"], view["constraints"]);
    assert!(
        near(at(&read, b.as_str().unwrap()), [5.0, 3.0, 22.0]),
        "{read}"
    );
    let saved_again: Value = serde_json::from_str(&back.to_json()).unwrap();
    assert_eq!(saved_again["features"], saved["features"]);
    assert!(
        item(&read, "constraints", &made(&symmetric))["plane"]["construction"] == "F1",
        "{read}"
    );

    // A plane that comes after the sketch would make a link cycle or is
    // not there yet: refused, in words.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 40}}}),
    );
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": s, "constraint":
               {"type": "on_plane", "points": [a], "plane": {"construction": "F4"}}}),
    );
    assert!(!error.is_empty());
}

#[test]
fn a_driving_dimension_uses_a_driven_one_of_the_same_sketch() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let ab = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0],
               "end": [38, 1, 0], "axis": "x", "length": 40, "start_source": "origin"}),
    );
    // A driven Delta X of the first line: its reference parameter.
    let delta = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": "F1",
               "dimension": {"type": "offset", "a": ab["start"], "b": ab["end"], "axis": "x"},
               "driven": true}),
    );
    let view = sketch(&doc, "F1");
    let driven = item(&view, "dimensions", &made(&delta));
    let reference = driven["reference"]
        .as_str()
        .unwrap_or_else(|| panic!("{driven}"))
        .to_owned();
    assert!((driven["measured"].as_f64().unwrap() - 40.0).abs() <= TOL);
    // A second line from the first's end, half as long as it measures.
    let bc = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": ab["end"],
               "end": [40, 10, 0], "axis": "y", "length": format!("{reference} / 2")}),
    );
    let view = sketch(&doc, "F1");
    let end = bc["end"].as_str().unwrap();
    assert!(near(at(&view, end), [40.0, 20.0, 0.0]), "{view}");
    // The first line's length changes: the second follows in the same
    // recompute.
    command(
        &mut doc,
        json!({"cmd": "sketch3d.set_dimension", "sketch": "F1",
               "dimension": ab["dimensions"][0], "value": 60}),
    );
    let view = sketch(&doc, "F1");
    assert!(near(at(&view, end), [60.0, 30.0, 0.0]), "{view}");
    // Saved and read again: the reference and its use.
    let file = doc.to_json();
    let mut back = Document::from_json(&file, MockKernel::default()).unwrap();
    back.recompute();
    let read = sketch(&back, "F1");
    assert!(near(at(&read, end), [60.0, 30.0, 0.0]), "{read}");
    assert_eq!(
        item(&read, "dimensions", &made(&delta))["reference"],
        json!(reference)
    );

    // A driving dimension that moves what it uses: refused as a circular
    // dependency, nothing kept.
    let length = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": made(&bc)}, "driven": true}),
    );
    let view = sketch(&doc, "F1");
    let own = item(&view, "dimensions", &made(&length))["reference"]
        .as_str()
        .unwrap()
        .to_owned();
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.set_dimension", "sketch": "F1",
               "dimension": bc["dimensions"][0], "value": format!("{own} + 1")}),
    );
    assert!(error.contains("circular"), "{error}");
    assert_eq!(sketch(&doc, "F1")["dimensions"], view["dimensions"]);
}

#[test]
fn curvature_concentric_and_tangent_magnitude_through_the_api() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let line = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [-20, 0, 0],
               "end": [0, 0, 0], "axis": "x", "length": 20}),
    );
    let spline = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_spline", "sketch": "F1",
               "points": [line["end"], [10, 5, 0], [20, 0, 5], [30, 5, 5]]}),
    );
    let g2 = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": "F1", "constraint":
               {"type": "curvature", "a": made(&line), "b": made(&spline), "at": line["end"]}}),
    );
    let view = sketch(&doc, "F1");
    assert_eq!(
        item(&view, "constraints", &made(&g2))["sense"],
        "positive",
        "{view}"
    );
    // The start is held now: its tangent magnitude drives.
    let magnitude = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_magnitude", "curve": made(&spline), "end": "start"},
               "value": 45}),
    );
    let view = sketch(&doc, "F1");
    let measured = item(&view, "dimensions", &made(&magnitude))["measured"]
        .as_f64()
        .unwrap();
    assert!((measured - 45.0).abs() <= TOL, "{view}");
    // The natural end is refused in words.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_magnitude", "curve": made(&spline), "end": "end"},
               "value": 30}),
    );
    assert!(error.contains("natural end"), "{error}");
    // A driving spline length.
    let length = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": "F1",
               "dimension": {"type": "spline_length", "curve": made(&spline)}, "value": 50}),
    );
    let view = sketch(&doc, "F1");
    let measured = item(&view, "dimensions", &made(&length))["measured"]
        .as_f64()
        .unwrap();
    assert!((measured - 50.0).abs() <= TOL, "{view}");
    // Two circles made concentric, the centre only.
    let a = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_circle", "sketch": "F1", "center": [0, 40, 0], "radius": 5,
               "normal": [0, 0, 1]}),
    );
    let b = command(
        &mut doc,
        json!({"cmd": "sketch3d.add_circle", "sketch": "F1", "center": [3, 44, 6], "radius": 9,
               "normal": [1, 0, 1]}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": "F1", "constraint":
               {"type": "concentric", "a": made(&a), "b": made(&b), "normal": false}}),
    );
    let view = sketch(&doc, "F1");
    let centre = |c: &Value| {
        item(&view, "curves", &made(c))["center"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert!(
        near(at(&view, &centre(&a)), at(&view, &centre(&b))),
        "{view}"
    );
}
