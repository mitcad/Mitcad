// SPDX-License-Identifier: MIT
//! A 3D sketch dimension's value that uses the dimension's own parameter
//! keeps that parameter as a user parameter, as a planar sketch's
//! dimension does (mitcad#1535, the same rule and code): `Reach * 2` typed
//! into the length of Reach keeps Reach, with its value, as a user
//! parameter and gives the dimension a parameter of its own with that
//! expression; the bare name makes the dimension use the user parameter.
//! One command, one undo step.

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
    query(doc, json!({"query": "sketch3d", "uid": "F1"}))["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no dimension {id}"))
}

fn measured(doc: &Document<MockKernel>, id: &str) -> f64 {
    let d = dimension(doc, id);
    d["measured"]
        .as_f64()
        .unwrap_or_else(|| panic!("{id} measures nothing: {d}"))
}

fn set_dimension(dimension: &str, value: &str) -> Value {
    json!({"cmd": "sketch3d.set_dimension", "sketch": "F1", "dimension": dimension,
           "value": value})
}

/// 3DSketch1 (F1) with a line from the origin 40 mm along +X, its length
/// Reach = 40 mm (owned by the sketch), and from its end a line 30 mm
/// along +Y; a user parameter Clearance = Reach + 1 mm. Returns the
/// document and the two length dimensions.
fn reach() -> (Document<MockKernel>, String, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch3d.create"}));
    let first = command(
        &mut d,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0],
               "end": [37, 2, 1], "axis": "x", "length": 40, "start_source": "origin"}),
    );
    let second = command(
        &mut d,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": first["end"],
               "end": [42, 27, 0], "axis": "y", "length": 30}),
    );
    let along = first["dimensions"][0].as_str().unwrap().to_owned();
    let across = second["dimensions"][0].as_str().unwrap().to_owned();
    command(&mut d, set_dimension(&along, "Reach = 40 mm"));
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "Clearance", "value": "Reach + 1 mm"}),
    );
    let own = parameter(&d, "Reach");
    assert_eq!(own["kind"], "model", "{own}");
    assert_eq!(own["owner"], "F1", "{own}");
    assert_eq!(dimension(&d, &along)["value"], "Reach");
    (d, along, across)
}

#[test]
fn an_expression_of_the_own_parameter_keeps_it_as_a_user_parameter() {
    let (mut d, along, _) = reach();
    let before = parameters(&d);
    let result = command(&mut d, set_dimension(&along, "Reach * 2"));
    // Reach stays, with its expression and value, a user parameter now.
    let kept = parameter(&d, "Reach");
    assert_eq!(kept["kind"], "user", "{kept}");
    assert_eq!(kept["expression"], "40 mm", "{kept}");
    assert!(
        (kept["value"].as_f64().unwrap() - 40.0).abs() < 1e-6,
        "{kept}"
    );
    assert!(kept["owner"].is_null(), "{kept}");
    // The dimension: 80 mm, a generated parameter of the sketch.
    assert!((measured(&d, &along) - 80.0).abs() < 1e-6);
    let now = dimension(&d, &along)["value"].as_str().unwrap().to_owned();
    assert!(now.starts_with('d') && now != "Reach", "{now}");
    let generated = parameter(&d, &now);
    assert_eq!(generated["expression"], "Reach * 2", "{generated}");
    assert_eq!(generated["kind"], "model", "{generated}");
    assert_eq!(generated["owner"], "F1", "{generated}");
    // The other expression refers to the same name and value.
    let clearance = parameter(&d, "Clearance");
    assert_eq!(clearance["expression"], "Reach + 1 mm");
    assert!((clearance["value"].as_f64().unwrap() - 41.0).abs() < 1e-6);
    // The result says what moved, as the planar command's does.
    assert_eq!(
        result["user_parameters"],
        json!([{"name": "Reach", "dimension": along, "parameter": now}]),
        "{result}"
    );
    let notes = result["notes"].to_string();
    assert!(notes.contains("Reach is now a user parameter"), "{result}");
    // One undo step back.
    command(&mut d, json!({"cmd": "undo"}));
    assert_eq!(parameters(&d), before);
    assert_eq!(dimension(&d, &along)["value"], "Reach");
    assert!((measured(&d, &along) - 40.0).abs() < 1e-6);
}

#[test]
fn a_value_using_the_own_parameter_through_another_or_named_keeps_it_too() {
    // Through Clearance, which uses Reach: 2 * 41 mm.
    let (mut d, along, _) = reach();
    command(&mut d, set_dimension(&along, "Clearance * 2"));
    assert_eq!(parameter(&d, "Reach")["kind"], "user");
    assert!((measured(&d, &along) - 82.0).abs() < 1e-6);
    // `name = expression` with another name: that name for the dimension.
    let (mut d, along, _) = reach();
    command(&mut d, set_dimension(&along, "Span = Reach * 2"));
    assert_eq!(parameter(&d, "Reach")["kind"], "user");
    assert_eq!(dimension(&d, &along)["value"], "Span");
    assert!((measured(&d, &along) - 80.0).abs() < 1e-6);
    assert_eq!(parameter(&d, "Span")["owner"], "F1");
}

#[test]
fn the_bare_own_name_moves_the_parameter_to_the_user_parameters() {
    let (mut d, along, _) = reach();
    let count = parameters(&d).len();
    let result = command(&mut d, set_dimension(&along, "Reach"));
    let kept = parameter(&d, "Reach");
    assert_eq!(kept["kind"], "user", "{kept}");
    assert_eq!(dimension(&d, &along)["value"], "Reach");
    assert!((measured(&d, &along) - 40.0).abs() < 1e-6);
    // No new parameter.
    assert_eq!(parameters(&d).len(), count);
    assert_eq!(
        result["user_parameters"],
        json!([{"name": "Reach", "dimension": along, "parameter": "Reach"}]),
        "{result}"
    );
    // The user parameter drives the dimension.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "Reach", "expression": "45 mm"}),
    );
    assert!((measured(&d, &along) - 45.0).abs() < 1e-6);
}

#[test]
fn self_references_that_cannot_be_kept_this_way_are_still_cycles() {
    let (mut d, along, across) = reach();
    let before = parameters(&d);
    // The own name renamed to itself keeps meaning "name and set it".
    let error = rejected(&mut d, set_dimension(&along, "Reach = Reach * 2"));
    assert!(error.contains("circular reference"), "{error}");
    assert_eq!(parameters(&d), before);
    // A parameter another dimension of the sketch uses as well stays with
    // the sketch: this dimension only gets a parameter of its own.
    command(&mut d, set_dimension(&across, "Reach"));
    command(&mut d, set_dimension(&along, "Reach * 2"));
    let own = parameter(&d, "Reach");
    assert_eq!(own["kind"], "model", "{own}");
    assert_eq!(dimension(&d, &across)["value"], "Reach");
    assert!((measured(&d, &along) - 80.0).abs() < 1e-6);
    assert!((measured(&d, &across) - 40.0).abs() < 1e-6);
}

#[test]
fn a_new_dimension_or_a_driven_one_made_driving_moves_nothing() {
    // A dimension without a parameter of its own yet takes the value as
    // it is: a driven one made driving by a value that names a user
    // parameter only uses it.
    let (mut d, _, across) = reach();
    command(
        &mut d,
        json!({"cmd": "sketch3d.set_driven", "sketch": "F1", "dimension": across,
               "driven": true}),
    );
    let result = command(&mut d, set_dimension(&across, "Clearance - 11 mm"));
    assert!(result.get("user_parameters").is_none(), "{result}");
    assert_eq!(parameter(&d, "Reach")["kind"], "model");
    assert!((measured(&d, &across) - 30.0).abs() < 1e-6);
}
