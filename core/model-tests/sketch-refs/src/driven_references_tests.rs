// SPDX-License-Identifier: MIT
//! Driven dimensions as read-only reference parameters (mitcad#231, part
//! B): the value a driven dimension measures, usable after its sketch.

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

fn parameter(doc: &Document<MockKernel>, name: &str) -> Option<Value> {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
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

fn close(a: &Value, b: f64) -> bool {
    a.as_f64().is_some_and(|a| (a - b).abs() < 1e-9)
}

/// Sketch1 (F1) with a 60 x 40 rectangle (width k5 d1, height k6 d2) and
/// a driven length of its left side; returns the driven dimension's id.
fn rectangle() -> (Document<MockKernel>, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": "c4"}, "driven": true}),
    );
    assert_eq!(added["parameters"], json!(["d3"]));
    (d, added["dimensions"][0].as_str().unwrap().to_owned())
}

fn set_dimension(dimension: &str, value: &str) -> Value {
    json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension, "value": value})
}

/// An offset plane after Sketch1 (F2) whose distance is `distance`.
fn plane(doc: &mut Document<MockKernel>, distance: &str) {
    command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": distance}}}),
    );
}

#[test]
fn a_driven_dimension_gets_a_reference_that_follows_the_sketch() {
    let (mut d, k) = rectangle();
    let k7 = dimension(&d, &k);
    assert_eq!(k7["driven"], true);
    assert_eq!(k7["reference"], "d3");
    assert_eq!(k7["comment"], "Sketch1 length (driven)");
    let d3 = parameter(&d, "d3").unwrap();
    assert_eq!(d3["kind"], "reference");
    assert_eq!(d3["owner"], "F1");
    assert!(close(&d3["value"], 40.0), "{d3}");
    // An edit of the sketch and a parameter change both update it.
    command(&mut d, set_dimension("k6", "30"));
    assert!(close(&parameter(&d, "d3").unwrap()["value"], 30.0));
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d2", "value": "25 mm"}),
    );
    assert!(close(&parameter(&d, "d3").unwrap()["value"], 25.0));
    // A user parameter and a later feature use it.
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "twice", "value": "d3 * 2"}),
    );
    plane(&mut d, "d3 + 1 mm");
    assert!(close(&parameter(&d, "twice").unwrap()["value"], 50.0));
    command(&mut d, set_dimension("k6", "20"));
    assert!(close(&parameter(&d, "twice").unwrap()["value"], 40.0));
    let f2 = query(&d, json!({"query": "feature", "uid": "F2"}));
    assert_eq!(f2["status"], "ok", "{f2}");
    // Undo takes the value back with the edit.
    d.undo().unwrap();
    assert!(close(&parameter(&d, "d3").unwrap()["value"], 25.0));
}

#[test]
fn a_reference_is_renamed_but_never_set() {
    let (mut d, k) = rectangle();
    command(
        &mut d,
        json!({"cmd": "rename_parameter", "name": "d3", "new_name": "side"}),
    );
    assert_eq!(dimension(&d, &k)["reference"], "side");
    for change in [json!({"value": 10}), json!({"expression": "d1 / 2"})] {
        let mut cmd = json!({"cmd": "set_parameter", "name": "side"});
        cmd.as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        let error = rejected(&mut d, cmd);
        assert!(
            error.contains("parameter side is") && error.contains("cannot be set"),
            "{error}"
        );
    }
}

#[test]
fn driven_and_driving_again_keep_the_parameter() {
    let (mut d, _) = rectangle();
    // The height's own parameter becomes its reference, keeping its name.
    command(
        &mut d,
        json!({"cmd": "sketch.set_driven", "sketch": "F1", "dimension": "k6", "driven": true}),
    );
    let k6 = dimension(&d, "k6");
    assert_eq!(k6["reference"], "d2");
    let d2 = parameter(&d, "d2").unwrap();
    assert_eq!(d2["kind"], "reference");
    assert_eq!(d2["comment"], "Sketch1 height (driven)");
    assert!(close(&d2["value"], 40.0));
    // Setting it makes it driving: a model parameter of the same name.
    command(&mut d, set_dimension("k6", "35"));
    let k6 = dimension(&d, "k6");
    assert_eq!(k6["value"], "d2");
    assert!(k6.get("reference").is_none(), "{k6}");
    let d2 = parameter(&d, "d2").unwrap();
    assert_eq!(d2["kind"], "model");
    assert_eq!(d2["comment"], "Sketch1 height");
    assert!(close(&d2["value"], 35.0));
    assert!(close(&parameter(&d, "d3").unwrap()["value"], 35.0));
}

#[test]
fn a_reference_is_used_only_after_its_sketch() {
    let (mut d, _) = rectangle();
    // In its own sketch, only where it cannot change what it measures
    // (mitcad#1116, sketch_driven_uses_tests.rs): the height measures
    // itself.
    let error = rejected(&mut d, set_dimension("k6", "d3 * 2"));
    assert!(error.contains("would depend on itself"), "{error}");
    // A plane after the sketch may; moved before it, it may not.
    plane(&mut d, "d3");
    let error = rejected(
        &mut d,
        json!({"cmd": "reorder_feature", "uid": "F2", "index": 0}),
    );
    assert!(
        error.contains("d3 is what") && error.contains("Plane1, before Sketch1"),
        "{error}"
    );
    let can = query(&d, json!({"query": "can_reorder", "uid": "F2", "index": 0}));
    assert_eq!(can["ok"], false, "{can}");
    // A plane before the sketch, directly or through a parameter.
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "half", "value": "10 mm"}),
    );
    plane(&mut d, "half");
    command(
        &mut d,
        json!({"cmd": "reorder_feature", "uid": "F3", "index": 0}),
    );
    let error = rejected(
        &mut d,
        json!({"cmd": "set_parameter", "name": "half", "expression": "d3 / 2"}),
    );
    assert!(
        error.contains("d3 is what the driven dimension")
            && error.contains("through parameter half"),
        "{error}"
    );
    let error = rejected(
        &mut d,
        json!({"cmd": "edit_feature", "uid": "F3", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": "d3"}}}),
    );
    assert!(
        error.contains("d3 is what") && error.contains("before Sketch1"),
        "{error}"
    );
    // A user parameter no feature uses may.
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "free", "value": "d3 / 2"}),
    );
}

#[test]
fn deleting_the_dimension_or_sketch_leaves_used_references_as_numbers() {
    let (mut d, k) = rectangle();
    let remove = json!({"cmd": "sketch.remove", "sketch": "F1", "items": [k]});
    // Unused, it goes with its dimension.
    command(&mut d, remove.clone());
    assert!(parameter(&d, "d3").is_none());
    d.undo().unwrap();
    // Used, it keeps its last value as a user parameter.
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "twice", "value": "d3 * 2"}),
    );
    command(&mut d, remove);
    let d3 = parameter(&d, "d3").unwrap();
    assert_eq!(d3["kind"], "user");
    assert_eq!(d3["expression"], "40 mm");
    assert!(close(&parameter(&d, "twice").unwrap()["value"], 80.0));
    d.undo().unwrap();
    command(&mut d, json!({"cmd": "delete_feature", "uid": "F1"}));
    let d3 = parameter(&d, "d3").unwrap();
    assert_eq!(d3["kind"], "user");
    assert_eq!(d3["expression"], "40 mm");
}

/// Removes every driven dimension's `reference` and the parameter `name`,
/// as a file written before references.
fn without_references(value: &mut Value, name: &str) {
    match value {
        Value::Object(map) => {
            if map.get("driven") == Some(&Value::Bool(true)) {
                map.remove("reference");
            }
            if let Some(Value::Array(params)) = map.get_mut("parameters") {
                params.retain(|p| p.get("name").is_none_or(|n| n != name));
            }
            for v in map.values_mut() {
                without_references(v, name);
            }
        }
        Value::Array(items) => {
            for v in items {
                without_references(v, name);
            }
        }
        _ => {}
    }
}

#[test]
fn references_are_saved_and_older_files_get_them() {
    let (mut d, k) = rectangle();
    command(
        &mut d,
        json!({"cmd": "rename_parameter", "name": "d3", "new_name": "side"}),
    );
    let saved = d.to_json();
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(dimension(&loaded, &k)["reference"], "side");
    assert_eq!(parameter(&loaded, "side").unwrap()["kind"], "reference");
    let set = json!({"cmd": "set_parameter", "name": "side", "value": 5});
    let mut loaded = loaded;
    assert!(rejected(&mut loaded, set).contains("cannot be set"));
    // A file without references gets them on load.
    let mut old: Value = serde_json::from_str(&saved).unwrap();
    without_references(&mut old, "side");
    let mut old = Document::from_json(&old.to_string(), MockKernel::default()).unwrap();
    old.recompute();
    assert_eq!(dimension(&old, &k)["reference"], "d3");
    let d3 = parameter(&old, "d3").unwrap();
    assert_eq!(d3["kind"], "reference");
    assert!(close(&d3["value"], 40.0), "{d3}");
}

/// Adds a sketch on XY with a fixed 30 mm line and a driven length k4 of
/// it whose `reference` names the parameter `name`.
fn sketch_naming(name: &str) -> Value {
    json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
        "entities": [
            {"id": "c1", "type": "line", "start": "p2", "end": "p3"},
            {"id": "p2", "type": "point", "at": [0, 0], "fixed": true},
            {"id": "p3", "type": "point", "at": [30, 0], "fixed": true}],
        "dimensions": [{"id": "k4", "type": "length", "line": "c1", "driven": true,
                        "reference": name}]}})
}

/// A definition's driven dimension whose `reference` names a user parameter
/// takes that parameter as its reference (mitcad#250), as an importer maps
/// a file's parameter of a driven dimension: owned by the sketch,
/// read-only, the value the sketch measures; no `d<n>` of its own.
#[test]
fn a_definition_takes_a_user_parameter_as_a_reference() {
    let mut d = Document::new(MockKernel::default());
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "side", "value": "7 mm"}),
    );
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "twice", "value": "side * 2"}),
    );
    command(&mut d, sketch_naming("side"));
    assert_eq!(dimension(&d, "k4")["reference"], "side");
    let side = parameter(&d, "side").unwrap();
    assert_eq!(side["kind"], "reference");
    assert_eq!(side["owner"], "F1");
    assert_eq!(side["comment"], "Sketch1 length (driven)");
    assert!(close(&side["value"], 30.0), "{side}");
    assert!(parameter(&d, "d1").is_none());
    assert!(close(&parameter(&d, "twice").unwrap()["value"], 60.0));
    let set = json!({"cmd": "set_parameter", "name": "side", "value": 5});
    assert!(rejected(&mut d, set).contains("cannot be set"));
    // Undo gives the user parameter back.
    d.undo().unwrap();
    let side = parameter(&d, "side").unwrap();
    assert_eq!(side["kind"], "user");
    assert!(close(&side["value"], 7.0), "{side}");
}

/// A user parameter that a feature before the sketch uses cannot become the
/// sketch's reference: the definition is refused, naming it (an importer
/// then adds the sketch without the references, mitcad#250).
#[test]
fn a_user_parameter_an_earlier_feature_uses_is_no_reference() {
    let mut d = Document::new(MockKernel::default());
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "side", "value": "7 mm"}),
    );
    plane(&mut d, "side");
    let error = rejected(&mut d, sketch_naming("side"));
    assert!(error.contains("side is what"), "{error}");
    assert_eq!(parameter(&d, "side").unwrap()["kind"], "user");
}
