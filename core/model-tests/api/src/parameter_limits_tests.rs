// SPDX-License-Identifier: MIT
//! Parameter limits (mitcad#665): min, max and step on any parameter,
//! clamped by default or refused when strict, in commands, queries, the
//! design file, undo, configurations and the features' own limits.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::parameters::slot_limits;
use mitcad_model::test_support::{block, def};
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

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(error) => error.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn parameter(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "parameters"}))
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

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// Wall = 4 mm; Width = 372 mm limited to [Wall * 2, 300 mm].
fn walls() -> Document<MockKernel> {
    let mut doc = doc();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Wall", "expression": "4 mm"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Width", "expression": "372 mm",
               "min": "Wall * 2", "max": 300}),
    );
    doc
}

#[test]
fn values_are_clamped_at_the_minimum_and_the_maximum() {
    let mut doc = walls();
    let width = parameter(&doc, "Width");
    assert_eq!(width["value"], 300.0);
    assert_eq!(width["text"], "300 mm");
    assert_eq!(width["expression"], "372 mm");
    assert_eq!(width["limited"], "max");
    assert_eq!(width["unclamped"], 372.0);
    assert_eq!(width["unclamped_text"], "372 mm");
    assert_eq!(
        width["limit_message"],
        "Width is limited to its maximum 300 mm (its expression gives 372 mm)"
    );
    let limits = &width["limits"];
    assert_eq!(
        (
            &limits["min"],
            &limits["max"],
            &limits["step"],
            &limits["strict"]
        ),
        (
            &json!("Wall * 2"),
            &json!("300"),
            &Value::Null,
            &json!(false)
        )
    );
    assert_eq!(limits["min_value"], 8.0);
    assert_eq!(limits["min_text"], "8 mm");
    assert_eq!(limits["max_value"], 300.0);
    assert_eq!(limits["max_number"], 300.0);
    assert_eq!(limits["step_value"], Value::Null);
    // The limit's references are dependencies too.
    assert_eq!(width["limit_references"], json!(["Wall"]));
    assert_eq!(width["dependencies"], json!(["Wall"]));
    // A parameter without limits has none of the fields.
    let wall = parameter(&doc, "Wall");
    assert!(wall.get("limits").is_none() && wall.get("limited").is_none());
    // The model's warnings list the limited parameters.
    let document = query(&doc, json!({"query": "document"}));
    assert_eq!(document["limited_parameters"][0]["name"], "Width");
    assert_eq!(document["limited_parameters"][0]["limited"], "max");

    // Below the minimum.
    let changed = command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "expression": "2 mm"}),
    );
    assert_eq!(changed["changed"], json!(["Width"]));
    assert_eq!(value(&doc, "Width"), 8.0);
    assert_eq!(parameter(&doc, "Width")["limited"], "min");
    // Inside: not limited, and the document lists nothing.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "value": 120}),
    );
    assert_eq!(value(&doc, "Width"), 120.0);
    assert_eq!(parameter(&doc, "Width")["limited"], Value::Null);
    assert!(
        query(&doc, json!({"query": "document"}))
            .get("limited_parameters")
            .is_none()
    );
    // A number above the maximum is kept as the expression, the value
    // limited; setting the value it already has is still a change.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "value": 400}),
    );
    assert_eq!(parameter(&doc, "Width")["expression"], "400 mm");
    assert_eq!(value(&doc, "Width"), 300.0);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "value": 300}),
    );
    assert_eq!(parameter(&doc, "Width")["expression"], "300 mm");
    assert_eq!(parameter(&doc, "Width")["limited"], Value::Null);
}

#[test]
fn limits_follow_the_parameters_they_reference() {
    let mut doc = walls();
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "expression": "10 mm"}),
    );
    assert_eq!(value(&doc, "Width"), 10.0);
    // Wall * 2 = 20 mm is now above Width's result: Width follows Wall.
    let changed = command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Wall", "expression": "10 mm"}),
    );
    assert_eq!(changed["changed"], json!(["Wall", "Width"]));
    assert_eq!(value(&doc, "Width"), 20.0);
    assert_eq!(parameter(&doc, "Width")["limited"], "min");
    // A rename rewrites the limit; a parameter a limit uses is not deleted.
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "Wall", "new_name": "Shell"}),
    );
    assert_eq!(parameter(&doc, "Width")["limits"]["min"], "Shell * 2");
    let error = refused(
        &mut doc,
        json!({"cmd": "delete_parameter", "name": "Shell"}),
    );
    assert!(error.contains("used by Width"), "{error}");
}

#[test]
fn values_snap_to_the_step() {
    let mut doc = walls();
    // Steps of 1 mm from the minimum (8 mm): 8, 9, 10, ...
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "expression": "33.3 mm", "step": 1}),
    );
    let width = parameter(&doc, "Width");
    assert_eq!(width["value"], 33.0);
    assert_eq!(width["limited"], "step");
    assert_eq!(width["limits"]["step_text"], "1 mm");
    assert_eq!(
        width["limit_message"],
        "Width is rounded to its step of 1 mm: 33 mm (its expression gives 33.3 mm)"
    );
    // Halfway goes up.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "expression": "33.5 mm"}),
    );
    assert_eq!(value(&doc, "Width"), 34.0);
    // Steps from a minimum off the grid: 8.5, 10.5, 12.5, ...
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "min": "8.5 mm", "step": "2 mm",
               "expression": "11 mm"}),
    );
    assert_eq!(value(&doc, "Width"), 10.5);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "expression": "11.5 mm"}),
    );
    assert_eq!(value(&doc, "Width"), 12.5);
    // A decimal step of a unitless count.
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "ratio", "expression": "0.33", "unit": "",
               "step": 0.1}),
    );
    assert!(near(value(&doc, "ratio"), 0.3));
    // Limits removed with null and "": the result of its expression
    // (11.5 mm) again.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "min": null, "step": ""}),
    );
    let width = parameter(&doc, "Width");
    assert_eq!(width["expression"], "11.5 mm");
    assert_eq!(width["value"], 11.5);
    assert_eq!(width["limits"]["min"], Value::Null);
    assert_eq!(width["limits"]["max"], "300");
    assert_eq!(width["limited"], Value::Null);
}

#[test]
fn cycles_through_limits_are_refused() {
    let mut doc = doc();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "a", "expression": "10 mm"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "b", "expression": "5 mm", "max": "a"}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "a", "expression": "b * 2"}),
    );
    assert!(error.contains("circular reference: a -> b -> a"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "a", "min": "b"}),
    );
    assert!(error.contains("circular reference: a -> b -> a"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "b", "min": "b / 2"}),
    );
    assert!(error.contains("circular reference: b -> b"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "c", "expression": "1 mm", "max": "c"}),
    );
    assert!(error.contains("circular reference: c -> c"), "{error}");
    // Limits of another kind, unknown names and a minimum above the maximum.
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "b", "min": "30 deg"}),
    );
    assert!(error.contains("parameter b: minimum:"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "b", "min": "nothing"}),
    );
    assert!(error.contains("parameter b: minimum:"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "b", "min": "20 mm"}),
    );
    assert!(
        error.contains("the minimum 20 mm is above the maximum 10 mm"),
        "{error}"
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "b", "step": -1}),
    );
    assert!(error.contains("the step must be positive"), "{error}");
    // Nothing changed.
    assert_eq!(parameter(&doc, "a")["dependencies"], json!([]));
    assert_eq!(parameter(&doc, "b")["limits"]["min"], Value::Null);
}

#[test]
fn strict_parameters_refuse_values_outside_their_limits() {
    let mut doc = doc();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "count", "expression": "3", "unit": "",
               "min": 1, "max": 10, "step": 1, "strict": true}),
    );
    assert_eq!(value(&doc, "count"), 3.0);
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "count", "expression": "12"}),
    );
    assert!(
        error.contains("parameter count: 12 is above the maximum 10"),
        "{error}"
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "count", "expression": "0"}),
    );
    assert!(error.contains("0 is below the minimum 1"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "count", "expression": "2.5"}),
    );
    assert!(error.contains("2.5 is not on a step of 1"), "{error}");
    assert_eq!(value(&doc, "count"), 3.0);
    assert_eq!(parameter(&doc, "count")["limits"]["strict"], true);
    // A strict parameter whose new limits leave its value out is refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "count", "max": 2}),
    );
    assert!(error.contains("3 is above the maximum 2"), "{error}");
    // New limits and a new expression are checked together.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "count", "max": 2, "expression": "2"}),
    );
    assert_eq!(value(&doc, "count"), 2.0);
    // Not strict: clamped.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "count", "strict": false, "expression": "7"}),
    );
    assert_eq!(value(&doc, "count"), 2.0);
    assert_eq!(parameter(&doc, "count")["limited"], "max");
}

#[test]
fn the_evaluate_query_says_what_the_limits_would_do() {
    let mut doc = walls();
    let answer = query(
        &doc,
        json!({"query": "evaluate", "expression": "500", "parameter": "Width"}),
    );
    assert_eq!(answer["value"], 500.0);
    assert_eq!(answer["text"], "500 mm");
    assert_eq!(answer["limited"], "max");
    assert_eq!(answer["limited_value"], 300.0);
    assert_eq!(answer["limited_text"], "300 mm");
    let answer = query(
        &doc,
        json!({"query": "evaluate", "expression": "Wall * 10", "parameter": "Width"}),
    );
    assert_eq!(answer["limited"], Value::Null);
    assert_eq!(answer["limited_value"], 40.0);
    // Strict: the query is refused with the reason.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "strict": true, "expression": "100 mm"}),
    );
    let error = doc
        .query(&json!({"query": "evaluate", "expression": "500", "parameter": "Width"}).to_string())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("500 mm is above the maximum 300 mm"),
        "{error}"
    );
    // Without a parameter the query is as before.
    let answer = query(&doc, json!({"query": "evaluate", "expression": "500"}));
    assert!(answer.get("limited").is_none());
}

#[test]
fn limits_are_saved_loaded_and_undone() {
    let mut doc = walls();
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "step": "0.5 mm", "strict": false}),
    );
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let width = &saved["parameters"][1];
    assert_eq!(width["name"], "Width");
    assert_eq!(width["min"], "Wall * 2");
    assert_eq!(width["max"], "300");
    assert_eq!(width["step"], "0.5 mm");
    assert_eq!(width.get("strict"), None);
    assert_eq!(width["value"], 300.0);
    // A parameter without limits saves none of the fields.
    assert_eq!(saved["parameters"][0].get("min"), None);
    let loaded = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(
        query(&loaded, json!({"query": "parameters"})),
        query(&doc, json!({"query": "parameters"}))
    );
    // Strict is saved too.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "strict": true, "expression": "250 mm"}),
    );
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(saved["parameters"][1]["strict"], true);
    let loaded = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(parameter(&loaded, "Width")["limits"]["strict"], json!(true));
    // A file whose strict parameter is outside its limits does not load.
    let mut bad = saved.clone();
    bad["parameters"][1]["expression"] = json!("400 mm");
    let error = Document::from_json(&bad.to_string(), MockKernel::default())
        .err()
        .expect("refused")
        .to_string();
    assert!(error.contains("parameters[1]"), "{error}");

    // Undo and redo.
    assert_eq!(doc.undo_label(), Some("Change Limits of Width"));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(parameter(&doc, "Width")["limits"]["strict"], false);
    assert_eq!(value(&doc, "Width"), 300.0);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(parameter(&doc, "Width")["limits"]["step"], Value::Null);
    assert_eq!(parameter(&doc, "Width")["limits"]["min"], "Wall * 2");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        query(&doc, json!({"query": "parameters"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    command(&mut doc, json!({"cmd": "redo"}));
    command(&mut doc, json!({"cmd": "redo"}));
    let width = parameter(&doc, "Width");
    assert_eq!(width["limits"]["min"], "Wall * 2");
    assert_eq!(width["limits"]["step"], "0.5 mm");
    assert_eq!(width["value"], 300.0);
    assert_eq!(doc.undo_label(), Some("Change Limits of Width"));
    // The same limits again are no command.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Width", "max": "300"}),
    );
    assert_eq!(doc.undo_label(), Some("Change Limits of Width"));
    assert_eq!(doc.undo_depth(), 3);
}

#[test]
fn configurations_keep_the_limits_of_their_parameters() {
    let mut doc = walls();
    command(
        &mut doc,
        json!({"cmd": "set_configurations", "configurations": {
            "parameters": ["Width"],
            "rows": [{"name": "Small", "values": {"Width": "50 mm"}},
                     {"name": "Huge", "values": {"Width": "900 mm"}}]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "apply_configuration", "name": "Huge"}),
    );
    let width = parameter(&doc, "Width");
    assert_eq!(width["expression"], "900 mm");
    assert_eq!(width["value"], 300.0);
    assert_eq!(width["limited"], "max");
    command(
        &mut doc,
        json!({"cmd": "apply_configuration", "name": "Small"}),
    );
    let width = parameter(&doc, "Width");
    assert_eq!(width["value"], 50.0);
    assert_eq!(width["limits"]["max"], "300");
}

#[test]
fn features_report_their_own_limits() {
    let mut b = block();
    let pattern = def(json!({"type": "rectangular_pattern",
        "objects": {"type": "bodies", "bodies": ["F2.b0"]},
        "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 3,
                       "distance": 80},
        "distance_type": "spacing"}));
    b.doc.add_feature(&pattern, None).unwrap();
    let params = query(&b.doc, json!({"query": "parameters"}));
    let limited: Vec<&Value> = params
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p.get("intrinsic").is_some())
        .collect();
    assert_eq!(limited.len(), 1, "{params}");
    let quantity = &limited[0]["intrinsic"][0];
    assert_eq!(quantity["feature"], "F3");
    assert_eq!(quantity["slot"], "direction1.quantity");
    assert_eq!(quantity["fixed"], true);
    assert_eq!(quantity["min_value"], 1.0);
    assert_eq!(quantity["max_value"], 10000.0);
    assert_eq!(quantity["step_value"], 1.0);
    // Only the features' own limits: the parameter has none of its own.
    assert!(limited[0].get("limits").is_none());

    assert!(slot_limits("circular_pattern", "quantity").is_some());
    assert!(slot_limits("extrude", "extent.distance").is_none());
    let teeth = slot_limits("gear", "teeth").unwrap();
    assert_eq!(
        (teeth.min, teeth.max, teeth.step),
        (Some(5.0), None, Some(1.0))
    );
    assert!(slot_limits("pulley", "teeth").is_none());
}

#[test]
fn a_limited_sketch_dimension_says_so() {
    let mut b = block();
    // d1, the rectangle's 60 mm width, at most 50 mm.
    command(
        &mut b.doc,
        json!({"cmd": "set_parameter", "name": "d1", "max": "50 mm"}),
    );
    assert_eq!(value(&b.doc, "d1"), 50.0);
    let sketch = query(&b.doc, json!({"query": "sketch", "uid": "F1"}));
    let width = sketch["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["value"] == "d1")
        .cloned()
        .unwrap_or_else(|| panic!("no d1 in {sketch}"));
    assert_eq!(width["limited"], "max");
    assert_eq!(width["measured"], 50.0);
    assert_eq!(
        width["limit_message"],
        "d1 is limited to its maximum 50 mm (its expression gives 60 mm)"
    );
}

#[test]
fn limits_of_an_unknown_parameter_are_refused() {
    let mut doc = walls();
    let error = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "nothing", "max": 3}),
    );
    assert!(error.contains("unknown parameter nothing"), "{error}");
}
