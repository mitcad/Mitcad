// SPDX-License-Identifier: MIT
//! Redefine Sketch Plane (mitcad#787, `sketch.redefine_plane`): a sketch
//! with an extrude moved to another plane, a face and a construction plane,
//! placed there by an offset, a move, a rotation and a flip; the extrude
//! follows, undo takes it back, the design file keeps it, and a plane made
//! after the sketch is refused. With the mock kernel, whose extrusions'
//! bounds follow their sketches' frames.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn vector(value: &Value) -> [f64; 3] {
    std::array::from_fn(|i| {
        value[i]
            .as_f64()
            .unwrap_or_else(|| panic!("no vector: {value}"))
    })
}

/// A sketch's evaluated frame: origin, x and y axes.
fn frame(doc: &Document<MockKernel>, sketch: &str) -> [[f64; 3]; 3] {
    let frame = query(doc, json!({"query": "sketch", "uid": sketch}))["frame"].clone();
    [
        vector(&frame["origin"]),
        vector(&frame["x_axis"]),
        vector(&frame["y_axis"]),
    ]
}

/// A body's box: its lowest and highest corner.
fn bounds(doc: &Document<MockKernel>, feature: u64) -> [[f64; 3]; 2] {
    let shape = doc
        .body_shape(BodyUid::new(FeatureUid(feature), 0))
        .unwrap_or_else(|| panic!("no body of F{feature}"));
    let b = shape.bounds.expect("bounds");
    [b.min, b.max]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= 1e-6)
}

#[track_caller]
fn assert_frame(got: [[f64; 3]; 3], origin: [f64; 3], x: [f64; 3], y: [f64; 3]) {
    assert!(
        near(got[0], origin) && near(got[1], x) && near(got[2], y),
        "frame {got:?}, expected origin {origin:?}, x {x:?}, y {y:?}"
    );
}

#[track_caller]
fn assert_bounds(got: [[f64; 3]; 2], min: [f64; 3], max: [f64; 3]) {
    assert!(
        near(got[0], min) && near(got[1], max),
        "bounds {got:?}, expected {min:?} to {max:?}"
    );
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

/// The sketch's definition as the `feature` query gives it.
fn sketch_def(doc: &Document<MockKernel>, sketch: &str) -> Value {
    query(doc, json!({"query": "feature", "uid": sketch}))["def"].clone()
}

/// A parameter's expression by name.
fn expression(doc: &Document<MockKernel>, name: &Value) -> String {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == *name)
        .unwrap_or_else(|| panic!("no parameter {name}"))["expression"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Sketch1 (F1) on XY with a 30 x 20 rectangle at the origin and
/// Extrude1 (F2) 10 up: Body1 (F2.b0) from (0, 0, 0) to (30, 20, 10).
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": R}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    assert_bounds(bounds(&doc, 2), [0.0, 0.0, 0.0], [30.0, 20.0, 10.0]);
    doc
}

#[test]
fn a_sketch_moves_to_another_plane_and_its_extrude_follows() {
    let mut doc = block();
    let before = undo_label(&doc);
    let result = command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1", "plane": "xz"}),
    );
    assert_eq!(result["plane"], "xz", "{result}");
    assert_eq!(result["plane_changed"], true, "{result}");
    assert!(result["placement"].is_null(), "{result}");
    assert_eq!(undo_label(&doc), "Redefine Plane of Sketch1");
    // The XZ plane's frame: x = +X, y = -Z, the normal +Y.
    assert_frame(
        frame(&doc, "F1"),
        [0.0; 3],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, -1.0],
    );
    // The geometry keeps its 2D coordinates; the extrusion goes along +Y.
    assert_bounds(bounds(&doc, 2), [0.0, 0.0, -20.0], [30.0, 10.0, 0.0]);

    // One undo step takes it back.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(undo_label(&doc), before);
    assert_eq!(sketch_def(&doc, "F1")["plane"], "xy");
    assert_frame(
        frame(&doc, "F1"),
        [0.0; 3],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
    );
    assert_bounds(bounds(&doc, 2), [0.0, 0.0, 0.0], [30.0, 20.0, 10.0]);
}

#[test]
fn offset_move_and_rotation_are_kept_as_parameters_and_change_again() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Lift", "expression": "5 mm", "unit": "mm"}),
    );
    let result = command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1",
               "offset": "Lift", "x": 10, "y": "2 * 3 mm", "angle": "90 deg"}),
    );
    assert_eq!(result["plane_changed"], false, "{result}");
    assert_eq!(undo_label(&doc), "Redefine Plane of Sketch1");
    // Moved by (10, 6), 5 up, turned a quarter: x = +Y, y = -X.
    assert_frame(
        frame(&doc, "F1"),
        [10.0, 6.0, 5.0],
        [0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0],
    );
    assert_bounds(bounds(&doc, 2), [-10.0, 6.0, 5.0], [10.0, 36.0, 15.0]);
    // Stored with the sketch: the borrowed parameter by name, the others
    // the sketch's own parameters with their expressions.
    let placement = sketch_def(&doc, "F1")["placement"].clone();
    assert_eq!(placement["offset"], "Lift", "{placement}");
    assert_eq!(expression(&doc, &placement["y"]), "2 * 3 mm");
    assert_eq!(expression(&doc, &placement["angle"]), "90 deg");
    assert_eq!(placement, result["placement"]);

    // A parameter moves it.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Lift", "expression": "8 mm"}),
    );
    assert!(near(frame(&doc, "F1")[0], [10.0, 6.0, 8.0]));

    // Changed again: only the values given; the rest stay.
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1", "x": "12 mm"}),
    );
    let again = sketch_def(&doc, "F1")["placement"].clone();
    assert_eq!(again["y"], placement["y"], "the same parameter");
    assert_eq!(expression(&doc, &again["x"]), "12 mm");
    assert_frame(
        frame(&doc, "F1"),
        [12.0, 6.0, 8.0],
        [0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0],
    );

    // Zeros take the values away, and their parameters go with them.
    let own: Vec<Value> = ["x", "y", "angle"]
        .iter()
        .map(|k| again[*k].clone())
        .collect();
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1",
               "offset": 0, "x": "0 mm", "y": 0, "angle": "0 deg"}),
    );
    assert!(sketch_def(&doc, "F1").get("placement").is_none());
    let names: Vec<Value> = query(&doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].clone())
        .collect();
    for name in &own {
        assert!(!names.contains(name), "{name} stays: {names:?}");
    }
    assert_bounds(bounds(&doc, 2), [0.0, 0.0, 0.0], [30.0, 20.0, 10.0]);
}

#[test]
fn a_flip_turns_the_sketch_over() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1", "flip": true}),
    );
    assert_frame(
        frame(&doc, "F1"),
        [0.0; 3],
        [1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
    );
    // Seen from below: the rectangle mirrored across x, extruded down.
    assert_bounds(bounds(&doc, 2), [0.0, -20.0, -10.0], [30.0, 0.0, 0.0]);
    assert_eq!(sketch_def(&doc, "F1")["placement"]["flip"], true);
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1", "flip": false}),
    );
    assert!(sketch_def(&doc, "F1").get("placement").is_none());
}

/// Block, then Plane1 (F3) 25 above XY, Sketch2 (F4) on XY with a 30 x 20
/// rectangle and Extrude2 (F5) 3 up: Body (F5.b0).
fn two_sketches() -> Document<MockKernel> {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {
               "type": "offset", "plane": "xy", "distance": 25}}}),
    );
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [0, 0], "width": 30, "height": 20}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F4", "region": R}],
               "extent": {"type": "distance", "distance": 3}, "operation": "new_body"}}),
    );
    assert_bounds(bounds(&doc, 5), [0.0, 0.0, 0.0], [30.0, 20.0, 3.0]);
    doc
}

#[test]
fn a_sketch_moves_onto_a_face_and_a_construction_plane_before_it() {
    let mut doc = two_sketches();
    let top = format!("F2:end({R})");
    // Body1's top face, 10 up, and 2 above it.
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F4",
               "plane": {"face": top, "body": "F2.b0"}, "offset": 2}),
    );
    assert_frame(
        frame(&doc, "F4"),
        [0.0, 0.0, 12.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
    );
    assert_bounds(bounds(&doc, 5), [0.0, 0.0, 12.0], [30.0, 20.0, 15.0]);
    assert_eq!(sketch_def(&doc, "F4")["plane"]["face"], top.as_str());

    // Plane1, the offset kept.
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F4", "plane": "F3"}),
    );
    assert_eq!(sketch_def(&doc, "F4")["plane"], "F3");
    assert_bounds(bounds(&doc, 5), [0.0, 0.0, 27.0], [30.0, 20.0, 30.0]);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_bounds(bounds(&doc, 5), [0.0, 0.0, 12.0], [30.0, 20.0, 15.0]);
}

#[test]
fn a_plane_made_after_the_sketch_is_refused_with_the_reason() {
    let mut doc = two_sketches();
    let before = undo_label(&doc);
    // Plane1 (F3) and Extrude2's faces come after Sketch1.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1", "plane": "F3"}),
    );
    assert!(
        error.contains("Sketch1 cannot lie on a plane or face of Plane1")
            && error.contains("Plane1 comes after Sketch1 in the timeline"),
        "{error}"
    );
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1",
               "plane": {"face": format!("F2:end({R})"), "body": "F2.b0"}}),
    );
    assert!(
        error.contains("Extrude1 comes after Sketch1 in the timeline"),
        "{error}"
    );
    // The document is as it was.
    assert_eq!(undo_label(&doc), before);
    assert_eq!(sketch_def(&doc, "F1")["plane"], "xy");
    // A face the sketch cannot lie on (the mock kernel's side faces are
    // not planar) is refused too.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F4",
               "plane": {"face": "F2:side(c1[c4,c2])", "body": "F2.b0"}}),
    );
    assert!(
        error.contains("Sketch2 cannot use this plane or face"),
        "{error}"
    );
    assert_eq!(undo_label(&doc), before);
    // Not a sketch.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F2", "plane": "xz"}),
    );
    assert!(error.contains("is not a sketch"), "{error}");
}

#[test]
fn an_upright_sketch_on_a_new_plane_is_one_step() {
    let mut doc = block();
    let before = undo_label(&doc);
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1", "plane": "xz", "upright": true}),
    );
    // As Create Sketch's: the design's +Z up, seen from the front
    // (mitcad#1060): x = +X, the normal -Y.
    assert_frame(
        frame(&doc, "F1"),
        [0.0; 3],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
    );
    assert!(sketch_def(&doc, "F1").get("frame").is_some());
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(undo_label(&doc), before);
    assert_eq!(sketch_def(&doc, "F1")["plane"], "xy");
    // The same plane again keeps the own frame.
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1", "plane": "xz", "upright": true}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F1", "plane": "xz", "offset": 4}),
    );
    // 4 along the upright frame's normal, x × y = (1, 0, 0) × (0, 0, 1),
    // that is -Y: towards the front.
    assert_frame(
        frame(&doc, "F1"),
        [0.0, -4.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
    );
}

#[test]
fn the_design_file_keeps_the_plane_and_placement() {
    let mut doc = two_sketches();
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F4", "plane": "F3",
               "offset": "-5 mm", "angle": "30 deg", "flip": true}),
    );
    let placed = frame(&doc, "F4");
    let body = bounds(&doc, 5);
    let json = doc.to_json();
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(loaded.to_json(), json, "saving again gives the same file");
    let again = frame(&loaded, "F4");
    for (a, b) in again.iter().zip(&placed) {
        assert!(near(*a, *b), "{again:?} after reading, {placed:?} before");
    }
    let reloaded = bounds(&loaded, 5);
    assert!(near(reloaded[0], body[0]) && near(reloaded[1], body[1]));
    assert_eq!(sketch_def(&loaded, "F4")["placement"]["flip"], true);

    // A file of an earlier version, without a placement, reads as before.
    let text = block().to_json();
    assert!(!text.contains("placement"), "{text}");
    let old = Document::from_json(&text, MockKernel::default()).unwrap();
    assert!(sketch_def(&old, "F1").get("placement").is_none());
}
