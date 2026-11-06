// SPDX-License-Identifier: MIT
//! Driven dimensions used by driving dimensions of their own sketch
//! (mitcad#1116): accepted when the user cannot move what the driven
//! dimension measures, solved after it, and refused as a circular
//! dependency otherwise.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

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

fn parameters(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "parameters"}))
}

fn parameter(doc: &Document<MockKernel>, name: &str) -> Value {
    parameters(doc)
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

fn value(doc: &Document<MockKernel>, name: &str) -> f64 {
    parameter(doc, name)["value"].as_f64().unwrap()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "sketch", "uid": "F1"}))
}

fn dimension(doc: &Document<MockKernel>, id: &str) -> Option<Value> {
    sketch(doc)["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .cloned()
}

/// The solved radius of circle `id` of Sketch1.
fn radius(doc: &Document<MockKernel>, id: &str) -> f64 {
    let entity = sketch(doc)["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no entity {id}"));
    entity["geometry"]["radius"]
        .as_f64()
        .unwrap_or_else(|| panic!("{entity}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn set_dimension(dimension: &str, value: &str) -> Value {
    json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension, "value": value})
}

fn driven_length(doc: &mut Document<MockKernel>, line: &str) -> (String, String) {
    let added = command(
        doc,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": line}, "driven": true}),
    );
    (
        added["dimensions"][0].as_str().unwrap().to_owned(),
        added["parameters"][0].as_str().unwrap().to_owned(),
    )
}

/// Sketch1 (F1): a `width` x `height` rectangle at the origin (lines
/// c1-c4, width k5 = d1 on the bottom c1, height k6 = d2 on the right side
/// c2) with a driven length of `line`, which is d3.
fn rectangle(width: f64, height: f64, line: &str) -> (Document<MockKernel>, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": width, "height": height}),
    );
    let (k, name) = driven_length(&mut d, line);
    assert_eq!(name, "d3");
    (d, k)
}

/// The minimal case: the top side of a 100 x 20 rectangle measured (d3),
/// and a separate circle's diameter `(d3 - d2) / 2`.
fn measured_circle() -> (Document<MockKernel>, String, String) {
    let (mut d, k) = rectangle(100.0, 20.0, "c3");
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [200, 0], "diameter": 10}),
    );
    let circle = added["curves"][0].as_str().unwrap().to_owned();
    let diameter = dimension(&d, "k8").expect("the circle's diameter is k8");
    assert_eq!(diameter["value"], "d4", "{diameter}");
    command(&mut d, set_dimension("k8", "(d3 - d2) / 2"));
    (d, k, circle)
}

#[test]
fn a_driving_dimension_uses_a_driven_one_of_its_sketch() {
    let (mut d, k, circle) = measured_circle();
    assert!(close(value(&d, "d3"), 100.0));
    assert!(close(value(&d, "d4"), 40.0));
    assert!(close(radius(&d, &circle), 20.0));
    // The measured dimension stays driven and read-only.
    let driven = dimension(&d, &k).unwrap();
    assert_eq!(driven["driven"], true, "{driven}");
    assert_eq!(parameter(&d, "d3")["kind"], "reference");
    let set = json!({"cmd": "set_parameter", "name": "d3", "value": 5});
    assert!(rejected(&mut d, set).contains("cannot be set"));
    // A later feature uses the dependent value.
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": "d4"}}}),
    );
    // The source changes: the measured value and its user follow.
    command(&mut d, set_dimension("k5", "120"));
    assert!(close(value(&d, "d3"), 120.0));
    assert!(close(value(&d, "d4"), 50.0));
    assert!(close(radius(&d, &circle), 25.0));
    assert_eq!(dimension(&d, &k).unwrap()["driven"], true);
    let plane = query(&d, json!({"query": "feature", "uid": "F2"}));
    assert_eq!(plane["status"], "ok", "{plane}");
    // So does a change of the independent value.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d2", "value": "40 mm"}),
    );
    assert!(close(value(&d, "d4"), 40.0));
    assert!(close(radius(&d, &circle), 20.0));
}

#[test]
fn undo_redo_and_reopening_keep_the_dependent_value() {
    let (mut d, _, circle) = measured_circle();
    command(&mut d, set_dimension("k5", "120"));
    d.undo().unwrap();
    assert!(close(value(&d, "d3"), 100.0));
    assert!(close(value(&d, "d4"), 40.0));
    assert!(close(radius(&d, &circle), 20.0));
    d.redo().unwrap();
    assert!(close(value(&d, "d3"), 120.0));
    assert!(close(value(&d, "d4"), 50.0));
    assert!(close(radius(&d, &circle), 25.0));
    // Saved and opened again: the expression and the reference stay.
    let saved = d.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(parameter(&loaded, "d3")["kind"], "reference");
    let d4 = parameter(&loaded, "d4");
    assert!(
        d4["expression"].as_str().is_some_and(|e| e.contains("d3")),
        "{d4}"
    );
    assert!(close(value(&loaded, "d4"), 50.0));
    assert!(close(radius(&loaded, &circle), 25.0));
    command(&mut loaded, set_dimension("k5", "90"));
    assert!(close(value(&loaded, "d3"), 90.0));
    assert!(close(value(&loaded, "d4"), 35.0));
    assert!(close(radius(&loaded, &circle), 17.5));
}

#[test]
fn measured_values_follow_each_other_in_stages() {
    let (mut d, _, circle) = measured_circle();
    // The circle's diameter measured (d5), and a second circle half that.
    let (_, measured) = driven_length_of_diameter(&mut d, &circle);
    assert_eq!(measured, "d5");
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [300, 0], "diameter": 10}),
    );
    let second = added["curves"][0].as_str().unwrap().to_owned();
    let k = added["dimensions"][0].as_str().unwrap().to_owned();
    command(&mut d, set_dimension(&k, "d5 / 2"));
    assert!(close(value(&d, "d5"), 40.0));
    assert!(close(radius(&d, &second), 10.0));
    command(&mut d, set_dimension("k5", "120"));
    assert!(close(value(&d, "d4"), 50.0));
    assert!(close(value(&d, "d5"), 50.0));
    assert!(close(radius(&d, &second), 12.5));
}

fn driven_length_of_diameter(doc: &mut Document<MockKernel>, circle: &str) -> (String, String) {
    let added = command(
        doc,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "diameter", "curve": circle}, "driven": true}),
    );
    (
        added["dimensions"][0].as_str().unwrap().to_owned(),
        added["parameters"][0].as_str().unwrap().to_owned(),
    )
}

/// Connected geometry is no cycle while the measured side is fixed without
/// its user: the left side of the rectangle has the height, whatever the
/// width is.
#[test]
fn connected_geometry_fixed_without_the_user_is_no_cycle() {
    let (mut d, _) = rectangle(60.0, 40.0, "c4");
    command(&mut d, set_dimension("k5", "d3 * 2"));
    assert!(close(value(&d, "d1"), 80.0));
    command(&mut d, set_dimension("k6", "30"));
    assert!(close(value(&d, "d3"), 30.0));
    assert!(close(value(&d, "d1"), 60.0));
    // Without the height the width would size the side it uses.
    let before = parameters(&d);
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": ["k6"]}),
    );
    assert!(
        error.contains("k5 (d1) uses d3")
            && error.contains("driven dimension k7 (d3)")
            && error.contains("would depend on itself"),
        "{error}"
    );
    assert_eq!(parameters(&d), before);
    assert!(dimension(&d, "k6").is_some());
}

/// A dimension that sizes what it uses is refused, naming both, and
/// nothing changes.
#[test]
fn a_dimension_that_moves_what_it_uses_is_refused() {
    let (mut d, k) = rectangle(60.0, 40.0, "c4");
    let before = parameters(&d);
    let depth = d.undo_depth();
    let error = rejected(&mut d, set_dimension("k6", "d3 * 2"));
    assert!(
        error.contains("Sketch1: k6 (d2) uses d3")
            && error.contains(&format!("driven dimension {k} (d3)"))
            && error.contains("would depend on itself")
            && error.contains("constrain that geometry fully"),
        "{error}"
    );
    assert_eq!(parameters(&d), before);
    assert_eq!(d.undo_depth(), depth);
    assert!(close(value(&d, "d3"), 40.0));
    // Through another parameter too.
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "twice", "value": "d3 * 2"}),
    );
    let error = rejected(&mut d, set_dimension("k6", "twice"));
    assert!(error.contains("would depend on itself"), "{error}");
}

/// Only the sketch's driving dimensions may use its references: an offset
/// distance of the same sketch may not.
#[test]
fn other_values_of_the_sketch_still_may_not_use_its_references() {
    let (mut d, _) = rectangle(60.0, 40.0, "c4");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1", "c2", "c3", "c4"],
               "distance": "d3 / 4"}),
    );
    assert!(
        error.contains("d3 is what") && error.contains("Sketch1 itself"),
        "{error}"
    );
}

/// The design saved as `d`, with the expression of parameter `name`
/// replaced, as a file written elsewhere may have it, opened and
/// recomputed.
fn opened_with(d: &Document<MockKernel>, name: &str, expression: &str) -> Document<MockKernel> {
    let mut file: Value = serde_json::from_str(&d.to_json()).unwrap();
    let parameter = file["parameters"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no parameter {name} in the file"));
    parameter["expression"] = json!(expression);
    let mut opened = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    opened.recompute();
    opened
}

/// Sketch1's error after a recompute.
fn sketch_error(d: &Document<MockKernel>) -> String {
    let feature = query(d, json!({"query": "feature", "uid": "F1"}));
    feature["error"]
        .as_str()
        .unwrap_or_else(|| panic!("Sketch1 does not fail: {feature}"))
        .to_owned()
}

/// The evaluation names every parameter in its errors (mitcad#1314): the
/// cycle the command refuses, found in a design opened from a file, names
/// the driven dimension's reference; a derived offset's distance set below
/// zero names that parameter.
#[test]
fn evaluation_errors_name_driven_references_and_offset_distances() {
    let (d, k) = rectangle(60.0, 40.0, "c4");
    assert_eq!(k, "k7");
    let opened = opened_with(&d, "d2", "d3 * 2");
    let error = sketch_error(&opened);
    assert!(
        error.contains("k6 (d2) uses d3")
            && error.contains("driven dimension k7 (d3)")
            && error.contains("so d3 would depend on itself"),
        "{error}"
    );
    // A derived offset (of an ellipse): its distance has no dimension.
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.ellipse", "sketch": "F1", "center": [0, 0], "major": [10, 0],
               "minor_radius": 5}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1"], "distance": 2}),
    );
    let s = sketch(&d);
    assert_eq!(s["offsets"][0]["derived"], true, "{s}");
    let distance = s["offsets"][0]["distance"].as_str().unwrap().to_owned();
    let opened = opened_with(&d, &distance, "-2");
    let error = sketch_error(&opened);
    assert!(
        error.contains(&format!("{distance} must be greater than zero")),
        "{error}"
    );
}
