// SPDX-License-Identifier: MIT
//! Transparent bodies and occurrences (mitcad#1093) with the mock kernel:
//! the commands and their undo steps, what the queries list, the design
//! file, and a hidden transparent occurrence that stays transparent.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

/// A 10 x 10 x 5 block (Sketch and Extrude) in the active component.
fn small_block(doc: &mut Document<MockKernel>) -> (FeatureUid, BodyUid) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (uid, BodyUid::new(uid, 0))
}

/// The `transparent` field of each placed body, by body and occurrence
/// path name (false when left out).
fn transparent_instances(doc: &Document<MockKernel>) -> Vec<(String, String, bool)> {
    query(doc, json!({"query": "instances", "hidden": true}))
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["body"].as_str().unwrap().to_owned(),
                i["occurrence"].as_str().unwrap().to_owned(),
                i["transparent"].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

/// A body's entry in the `bodies` query.
fn body_entry(doc: &Document<MockKernel>, uid: BodyUid) -> Value {
    query(doc, json!({"query": "bodies"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == json!(uid))
        .cloned()
        .unwrap()
}

/// A body's entry in a design file.
fn file_body(file: &Value, uid: BodyUid) -> Value {
    file["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == json!(uid))
        .cloned()
        .unwrap()
}

#[test]
fn transparency_of_bodies_and_occurrences_is_saved_and_undone() {
    let mut doc = Document::new(MockKernel::default());
    // The housing: a body of the root. The insert: a component placed once
    // (O1), with a body of its own.
    let (_, housing) = small_block(&mut doc);
    let (_, o1) = doc
        .create_component(Some("Insert"), Transform::IDENTITY, true)
        .unwrap();
    let (_, insert) = small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let housing_name = doc.body_name(housing);
    assert!(body_entry(&doc, housing).get("transparent").is_none());

    // The body: one undo step, listed by the queries.
    command(
        &mut doc,
        json!({"cmd": "set_body_transparent", "uid": housing, "transparent": true}),
    );
    assert_eq!(body_entry(&doc, housing)["transparent"], true);
    assert_eq!(
        transparent_instances(&doc),
        vec![
            (housing.to_string(), String::new(), true),
            (insert.to_string(), "Insert:1".to_owned(), false),
        ]
    );
    // The same again is no undo step.
    command(
        &mut doc,
        json!({"cmd": "set_body_transparent", "uid": housing, "transparent": true}),
    );

    // The occurrence, by its name: its contents are transparent.
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_transparent", "occurrence": "Insert:1", "transparent": true}),
    );
    let components = query(&doc, json!({"query": "components"}));
    assert_eq!(components["occurrences"][0]["transparent"], true);
    assert_eq!(
        transparent_instances(&doc),
        vec![
            (housing.to_string(), String::new(), true),
            (insert.to_string(), "Insert:1".to_owned(), true),
        ]
    );
    // Its own body is not marked: the occurrence carries it.
    assert!(body_entry(&doc, insert).get("transparent").is_none());

    // Hidden and shown again, it stays transparent.
    doc.set_occurrence_visible(o1, false).unwrap();
    assert_eq!(
        transparent_instances(&doc)[1],
        (insert.to_string(), "Insert:1".to_owned(), true)
    );
    doc.set_occurrence_visible(o1, true).unwrap();
    assert!(doc.instances().iter().all(|i| i.visible && i.transparent));

    // Saved and read back.
    let json = doc.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(file_body(&file, housing)["transparent"], true);
    assert!(file_body(&file, insert).get("transparent").is_none());
    assert_eq!(file["occurrences"][0]["transparent"], true);
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    loaded.recompute();
    assert!(loaded.instances().iter().all(|i| i.transparent));
    assert!(loaded.body_attributes(housing).transparent);

    // Undone step by step: Show, Hide, the occurrence, the body.
    assert_eq!(doc.undo().as_deref(), Some("Show Insert:1"));
    assert_eq!(doc.undo().as_deref(), Some("Hide Insert:1"));
    assert_eq!(doc.undo().as_deref(), Some("Make Insert:1 Transparent"));
    assert!(
        query(&doc, json!({"query": "components"}))["occurrences"][0]
            .get("transparent")
            .is_none()
    );
    assert_eq!(
        doc.undo().as_deref(),
        Some(format!("Make {housing_name} Transparent").as_str())
    );
    assert!(doc.instances().iter().all(|i| !i.transparent));
    // Nothing transparent is saved as before.
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(file_body(&file, housing).get("transparent").is_none());
    assert!(file["occurrences"][0].get("transparent").is_none());

    // Opaque again is its own step.
    doc.set_body_transparent(housing, true).unwrap();
    doc.set_body_transparent(housing, false).unwrap();
    assert_eq!(
        doc.undo().as_deref(),
        Some(format!("Make {housing_name} Opaque").as_str())
    );
    assert!(doc.body_attributes(housing).transparent);
}

#[test]
fn transparency_of_a_missing_body_or_occurrence_is_refused() {
    let mut doc = Document::new(MockKernel::default());
    small_block(&mut doc);
    for refused in [
        json!({"cmd": "set_body_transparent", "uid": "F9.b0", "transparent": true}),
        json!({"cmd": "set_occurrence_transparent", "occurrence": "O7", "transparent": true}),
        json!({"cmd": "set_body_transparent", "uid": "F2.b0"}),
    ] {
        assert!(doc.command(&refused.to_string()).is_err(), "{refused}");
    }
}
