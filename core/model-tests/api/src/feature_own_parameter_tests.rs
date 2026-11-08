// SPDX-License-Identifier: MIT
//! A feature value that uses the field's own parameter (mitcad#1536), as a
//! sketch dimension's does (mitcad#1535): `depth * 2` typed into the
//! distance of an extrusion whose distance is depth keeps depth, with its
//! value, as a user parameter and gives the distance a parameter of its own
//! with that expression, in the same `edit_feature` and undo step. The
//! bare own name leaves the slot as it is: definitions read back from the
//! model name their own parameters so.

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

fn assert_mm(p: &Value, mm: f64) {
    let value = p["value"].as_f64().unwrap();
    assert!((value - mm).abs() < 1e-6, "{value} mm, not {mm} mm: {p}");
}

/// The parameter extrusion `uid`'s distance uses.
fn distance(doc: &Document<MockKernel>, uid: &str) -> String {
    query(doc, json!({"query": "feature", "uid": uid}))["def"]["extent"]["distance"]
        .as_str()
        .unwrap_or_else(|| panic!("{uid} has no distance parameter"))
        .to_owned()
}

/// Sketch1 (F1) with a 60 x 40 rectangle; returns its region.
fn sketch(doc: &mut Document<MockKernel>) -> Value {
    command(doc, json!({"cmd": "sketch.create"}));
    command(
        doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    )["region"]
        .clone()
}

fn extrude(region: &Value, distance: &str) -> Value {
    json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
           "extent": {"type": "distance", "distance": distance}, "operation": "new_body"})
}

/// Extrude1 (F2) of the rectangle, its distance depth = 10 mm (its own).
fn extruded() -> (Document<MockKernel>, Value) {
    let mut doc = Document::new(MockKernel::default());
    let region = sketch(&mut doc);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": extrude(&region, "depth = 10 mm")}),
    );
    let own = parameter(&doc, "depth");
    assert_eq!(own["kind"], "model", "{own}");
    assert_eq!(own["owner"], "F2", "{own}");
    assert_eq!(distance(&doc, "F2"), "depth");
    (doc, region)
}

#[test]
fn an_expression_of_the_own_parameter_keeps_it_as_a_user_parameter() {
    let (mut doc, region) = extruded();
    let before = parameters(&doc);
    let result = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": extrude(&region, "depth * 2")}),
    );
    // depth stays, with its expression and value, a user parameter now.
    let kept = parameter(&doc, "depth");
    assert_eq!(kept["kind"], "user", "{kept}");
    assert_eq!(kept["expression"], "10 mm", "{kept}");
    assert!(kept["owner"].is_null(), "{kept}");
    assert_mm(&kept, 10.0);
    // The distance: 20 mm, a generated parameter of the extrusion.
    let now = distance(&doc, "F2");
    assert!(now.starts_with('d') && now != "depth", "{now}");
    let generated = parameter(&doc, &now);
    assert_eq!(generated["expression"], "depth * 2", "{generated}");
    assert_eq!(generated["kind"], "model", "{generated}");
    assert_eq!(generated["owner"], "F2", "{generated}");
    assert_mm(&generated, 20.0);
    // The result says what moved.
    assert_eq!(
        result["user_parameters"],
        json!([{"name": "depth", "slot": "extent.distance", "parameter": now}]),
        "{result}"
    );
    assert_eq!(
        result["notes"],
        json!([format!(
            "depth is now a user parameter (10 mm); Extrude1 distance is {now} = depth * 2"
        )]),
        "{result}"
    );
    // One undo step back.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(parameters(&doc), before);
    assert_eq!(distance(&doc, "F2"), "depth");
}

#[test]
fn the_bare_own_name_and_a_rename_leave_the_parameter_the_features() {
    // A definition read back from the model names the own parameter.
    let (mut doc, region) = extruded();
    let before = parameters(&doc);
    let result = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": extrude(&region, "depth")}),
    );
    assert!(result.get("user_parameters").is_none(), "{result}");
    assert_eq!(parameters(&doc), before);
    // `depth = …` names and sets the own parameter as before: a cycle.
    let error = doc
        .command(
            &json!({"cmd": "edit_feature", "uid": "F2",
                    "def": extrude(&region, "depth = depth * 2")})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("circular reference"), "{error}");
    assert_eq!(parameters(&doc), before);
    // Another name: that name for the distance.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": extrude(&region, "height = depth * 2")}),
    );
    assert_eq!(parameter(&doc, "depth")["kind"], "user");
    assert_eq!(distance(&doc, "F2"), "height");
    assert_eq!(parameter(&doc, "height")["owner"], "F2");
    assert_mm(&parameter(&doc, "height"), 20.0);
}

#[test]
fn an_own_parameter_another_feature_uses_stays_the_features() {
    let (mut doc, region) = extruded();
    // Extrude2 (F3) uses depth too.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": extrude(&region, "depth")}),
    );
    let result = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": extrude(&region, "depth * 2")}),
    );
    assert!(result.get("user_parameters").is_none(), "{result}");
    let own = parameter(&doc, "depth");
    assert_eq!(own["kind"], "model", "{own}");
    assert_eq!(own["owner"], "F2", "{own}");
    assert_eq!(distance(&doc, "F3"), "depth");
    let now = distance(&doc, "F2");
    assert_ne!(now, "depth");
    assert_mm(&parameter(&doc, &now), 20.0);
}

#[test]
fn a_preview_says_what_ok_will_keep_and_moves_nothing() {
    let (mut doc, region) = extruded();
    let before = parameters(&doc);
    let edit = json!({"cmd": "edit_feature", "uid": "F2", "def": extrude(&region, "depth * 2")});
    let report: Value = serde_json::from_str(&doc.preview(&edit.to_string()).unwrap()).unwrap();
    let infos = report["infos"].to_string();
    assert!(
        infos.contains("depth is now a user parameter (10 mm); Extrude1 distance is d"),
        "{report}"
    );
    assert_eq!(parameters(&doc), before);
    assert_eq!(distance(&doc, "F2"), "depth");
}
