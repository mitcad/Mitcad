// SPDX-License-Identifier: MIT
//! Remove Name (mitcad#1577): `remove_parameter_name` gives a dimension's
//! parameter its automatic name (`d<n>`) back, renamed as a rename renames
//! it: the expressions that use it and the configurations follow, in one
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

/// (name, expression) of every parameter.
fn parameters(doc: &Document<MockKernel>) -> Vec<(String, String)> {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap().to_owned(),
                p["expression"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn pair(name: &str, expression: &str) -> (String, String) {
    (name.to_owned(), expression.to_owned())
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

fn undo_depth(doc: &Document<MockKernel>) -> i64 {
    query(doc, json!({"query": "document"}))["undo_depth"]
        .as_i64()
        .unwrap()
}

/// Sketch1 (F1) with a 60 x 40 rectangle: width k5 (d1), height k6 (d2).
fn rectangle() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    d
}

fn set_dimension(dimension: &str, value: &str) -> Value {
    json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension, "value": value})
}

fn remove_name(name: &str) -> Value {
    json!({"cmd": "remove_parameter_name", "name": name})
}

#[test]
fn the_automatic_name_comes_back_and_the_uses_follow() {
    let mut d = rectangle();
    // The width named, the height and a user parameter using the name, and
    // a configuration of it.
    command(&mut d, set_dimension("k5", "width = 60"));
    command(&mut d, set_dimension("k6", "width / 2"));
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "gap", "value": "width + 1 mm"}),
    );
    command(
        &mut d,
        json!({"cmd": "set_configurations", "configurations": {"parameters": ["width"],
            "rows": [{"name": "Narrow", "values": {"width": "50 mm"}}]}}),
    );
    assert_eq!(dimension(&d, "k5")["value"], "width");
    assert_eq!(
        parameters(&d),
        [
            pair("width", "60"),
            pair("d2", "width / 2"),
            pair("gap", "width + 1 mm")
        ]
    );
    let depth = undo_depth(&d);

    // d1 is free again: the width takes it back.
    let result = command(&mut d, remove_name("width"));
    assert_eq!(result["name"], "d1");
    assert_eq!(result["rewritten"], json!(["d2", "gap"]));
    assert_eq!(
        parameters(&d),
        [
            pair("d1", "60"),
            pair("d2", "d1 / 2"),
            pair("gap", "d1 + 1 mm")
        ]
    );
    assert_eq!(dimension(&d, "k5")["value"], "d1");
    assert_eq!(dimension(&d, "k5")["measured"], 60.0);
    assert_eq!(dimension(&d, "k6")["measured"], 30.0);
    let table = query(&d, json!({"query": "configurations"}));
    assert_eq!(table["configurations"]["parameters"], json!(["d1"]));
    // One undo step, as a rename.
    assert_eq!(undo_depth(&d), depth + 1);
    // The values still follow the width through its new name.
    command(&mut d, set_dimension("k5", "70"));
    assert_eq!(dimension(&d, "k6")["measured"], 35.0);

    // Undo gives the name back, with the uses.
    command(&mut d, json!({"cmd": "undo"}));
    command(&mut d, json!({"cmd": "undo"}));
    assert_eq!(
        parameters(&d),
        [
            pair("width", "60"),
            pair("d2", "width / 2"),
            pair("gap", "width + 1 mm")
        ]
    );
    assert_eq!(dimension(&d, "k5")["value"], "width");
}

#[test]
fn the_next_free_automatic_name_is_taken() {
    let mut d = rectangle();
    command(&mut d, set_dimension("k5", "width = 60"));
    // A user parameter took d1 meanwhile: d2 is the height's, so d3.
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "d1", "value": "5 mm"}),
    );
    let result = command(&mut d, remove_name("width"));
    assert_eq!(result["name"], "d3");
    assert_eq!(result["rewritten"], json!([]));
    assert_eq!(dimension(&d, "k5")["value"], "d3");
    assert_eq!(dimension(&d, "k5")["measured"], 60.0);
}

#[test]
fn automatic_names_stay_and_user_parameters_are_refused() {
    let mut d = rectangle();
    let depth = undo_depth(&d);
    // Automatic already: nothing changes, and no undo step.
    let result = command(&mut d, remove_name("d2"));
    assert_eq!(result["name"], "d2");
    assert_eq!(undo_depth(&d), depth);
    // A user parameter has no automatic name.
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "gap", "value": "3 mm"}),
    );
    let before = parameters(&d);
    let error = rejected(&mut d, remove_name("gap"));
    assert!(error.contains("gap is a user parameter"), "{error}");
    let error = rejected(&mut d, remove_name("nothing"));
    assert!(error.contains("nothing"), "{error}");
    assert_eq!(parameters(&d), before);
}
