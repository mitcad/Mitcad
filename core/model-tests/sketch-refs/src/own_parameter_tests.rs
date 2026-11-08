// SPDX-License-Identifier: MIT
//! A dimension's value that uses the dimension's own parameter
//! (mitcad#1535): `BuckleDiameter * 2` typed into the dimension of
//! BuckleDiameter keeps BuckleDiameter, with its value, as a user parameter
//! and gives the dimension a parameter of its own with that expression; the
//! bare name makes the dimension use the user parameter. One command, one
//! undo step.

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

/// Every parameter as the `parameters` query gives it.
fn parameters(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .clone()
}

fn parameter(doc: &Document<MockKernel>, name: &str) -> Value {
    parameters(doc)
        .into_iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

fn dimension(doc: &Document<MockKernel>, id: &str) -> Value {
    query(doc, json!({"query": "sketch", "uid": "F1"}))["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .cloned()
        .unwrap()
}

fn set_dimension(dimension: &str, value: &str) -> Value {
    json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension, "value": value})
}

/// Sketch1 (F1) with a rectangle whose width k5 is BuckleDiameter = 5 mm
/// (owned by the sketch) and height k6 40 mm, and a user parameter
/// Clearance = BuckleDiameter + 1 mm.
fn buckle() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(&mut d, set_dimension("k5", "BuckleDiameter = 5 mm"));
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "Clearance", "value": "BuckleDiameter + 1 mm"}),
    );
    let own = parameter(&d, "BuckleDiameter");
    assert_eq!(own["kind"], "model", "{own}");
    assert_eq!(own["owner"], "F1", "{own}");
    d
}

#[test]
fn an_expression_of_the_own_parameter_keeps_it_as_a_user_parameter() {
    let mut d = buckle();
    let before = parameters(&d);
    let result = command(&mut d, set_dimension("k5", "BuckleDiameter * 2"));
    // BuckleDiameter stays, with its expression and value, a user
    // parameter now.
    let kept = parameter(&d, "BuckleDiameter");
    assert_eq!(kept["kind"], "user", "{kept}");
    assert_eq!(kept["expression"], "5 mm", "{kept}");
    assert!(
        (kept["value"].as_f64().unwrap() - 5.0).abs() < 1e-6,
        "{kept}"
    );
    assert!(kept["owner"].is_null(), "{kept}");
    // The dimension: 10 mm, a generated parameter of the sketch.
    let k5 = dimension(&d, "k5");
    assert!(
        (k5["measured"].as_f64().unwrap() - 10.0).abs() < 1e-6,
        "{k5}"
    );
    let now = k5["value"].as_str().unwrap().to_owned();
    assert!(now.starts_with('d') && now != "BuckleDiameter", "{k5}");
    let generated = parameter(&d, &now);
    assert_eq!(generated["expression"], "BuckleDiameter * 2", "{generated}");
    assert_eq!(generated["kind"], "model", "{generated}");
    assert_eq!(generated["owner"], "F1", "{generated}");
    // The other expression refers to the same name and value.
    let clearance = parameter(&d, "Clearance");
    assert_eq!(clearance["expression"], "BuckleDiameter + 1 mm");
    assert!((clearance["value"].as_f64().unwrap() - 6.0).abs() < 1e-6);
    // The result says what moved.
    assert_eq!(
        result["user_parameters"],
        json!([{"name": "BuckleDiameter", "dimension": "k5", "parameter": now}]),
        "{result}"
    );
    let notes = result["notes"].to_string();
    assert!(
        notes.contains("BuckleDiameter is now a user parameter"),
        "{result}"
    );
    // One undo step back.
    command(&mut d, json!({"cmd": "undo"}));
    assert_eq!(parameters(&d), before);
    let k5 = dimension(&d, "k5");
    assert_eq!(k5["value"], "BuckleDiameter");
    assert!(
        (k5["measured"].as_f64().unwrap() - 5.0).abs() < 1e-6,
        "{k5}"
    );
}

#[test]
fn a_value_using_the_own_parameter_through_another_or_named_keeps_it_too() {
    // Through Clearance, which uses BuckleDiameter: 2 * 6 mm.
    let mut d = buckle();
    command(&mut d, set_dimension("k5", "Clearance * 2"));
    assert_eq!(parameter(&d, "BuckleDiameter")["kind"], "user");
    let k5 = dimension(&d, "k5");
    assert!(
        (k5["measured"].as_f64().unwrap() - 12.0).abs() < 1e-6,
        "{k5}"
    );
    // `name = expression` with another name: that name for the dimension.
    let mut d = buckle();
    command(&mut d, set_dimension("k5", "Slot = BuckleDiameter * 2"));
    assert_eq!(parameter(&d, "BuckleDiameter")["kind"], "user");
    let k5 = dimension(&d, "k5");
    assert_eq!(k5["value"], "Slot");
    assert!(
        (k5["measured"].as_f64().unwrap() - 10.0).abs() < 1e-6,
        "{k5}"
    );
    assert_eq!(parameter(&d, "Slot")["owner"], "F1");
}

#[test]
fn the_bare_own_name_moves_the_parameter_to_the_user_parameters() {
    // What the dimension's context menu sends (Move BuckleDiameter to User
    // Parameters).
    let mut d = buckle();
    let count = parameters(&d).len();
    let result = command(&mut d, set_dimension("k5", "BuckleDiameter"));
    let kept = parameter(&d, "BuckleDiameter");
    assert_eq!(kept["kind"], "user", "{kept}");
    let k5 = dimension(&d, "k5");
    assert_eq!(k5["value"], "BuckleDiameter");
    assert!(
        (k5["measured"].as_f64().unwrap() - 5.0).abs() < 1e-6,
        "{k5}"
    );
    // No new parameter.
    assert_eq!(parameters(&d).len(), count);
    assert_eq!(
        result["user_parameters"],
        json!([{"name": "BuckleDiameter", "dimension": "k5", "parameter": "BuckleDiameter"}]),
        "{result}"
    );
    // The user parameter drives the dimension.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "BuckleDiameter", "expression": "7 mm"}),
    );
    let k5 = dimension(&d, "k5");
    assert!(
        (k5["measured"].as_f64().unwrap() - 7.0).abs() < 1e-6,
        "{k5}"
    );
}

#[test]
fn self_references_that_cannot_be_kept_this_way_are_still_cycles() {
    let mut d = buckle();
    let before = parameters(&d);
    // A user parameter's own expression.
    let error = rejected(
        &mut d,
        json!({"cmd": "set_parameter", "name": "Clearance", "expression": "Clearance * 2"}),
    );
    assert!(error.contains("circular reference"), "{error}");
    // The own name renamed to itself keeps meaning "name and set it".
    let error = rejected(
        &mut d,
        set_dimension("k5", "BuckleDiameter = BuckleDiameter * 2"),
    );
    assert!(error.contains("circular reference"), "{error}");
    assert_eq!(parameters(&d), before);
    // A parameter another dimension of the sketch uses as well stays with
    // the sketch: this dimension only gets a parameter of its own.
    command(&mut d, set_dimension("k6", "BuckleDiameter"));
    command(&mut d, set_dimension("k5", "BuckleDiameter * 2"));
    let own = parameter(&d, "BuckleDiameter");
    assert_eq!(own["kind"], "model", "{own}");
    assert_eq!(dimension(&d, "k6")["value"], "BuckleDiameter");
    let k5 = dimension(&d, "k5");
    assert!(
        (k5["measured"].as_f64().unwrap() - 10.0).abs() < 1e-6,
        "{k5}"
    );
}
