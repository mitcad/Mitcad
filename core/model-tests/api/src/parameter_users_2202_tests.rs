// SPDX-License-Identifier: MIT
//! What uses a user parameter (mitcad#2202): the `parameters` query's
//! `used_by` names, in words, what keeps a user parameter from being
//! deleted (the Parameters window disables its delete button with them),
//! and `delete_parameter` refuses exactly the parameters that have one.

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

fn parameter(doc: &Document<MockKernel>, name: &str) -> Option<Value> {
    let all: Value = serde_json::from_str(
        &doc.query(&json!({"query": "parameters"}).to_string())
            .unwrap(),
    )
    .unwrap();
    all.as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
}

fn used_by(doc: &Document<MockKernel>, name: &str) -> Vec<String> {
    let p = parameter(doc, name).unwrap_or_else(|| panic!("no parameter {name}"));
    p.get("used_by")
        .map(|users| {
            users
                .as_array()
                .unwrap()
                .iter()
                .map(|u| u.as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Sketch1 (F1) with a rectangle, the user parameters `free` (unused),
/// `len` and `twice = len * 2`, and Extrude1 (F2) whose distance is
/// `twice`.
fn design() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let region = command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    )["region"]
        .clone();
    for (name, expression) in [("free", "5 mm"), ("len", "10 mm"), ("twice", "len * 2")] {
        command(
            &mut doc,
            json!({"cmd": "add_parameter", "name": name, "expression": expression}),
        );
    }
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {
            "type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
            "extent": {"type": "distance", "distance": "twice"}, "operation": "new_body"}}),
    );
    doc
}

#[test]
fn used_by_names_what_uses_a_user_parameter() {
    let doc = design();
    assert!(
        used_by(&doc, "free").is_empty(),
        "{:?}",
        parameter(&doc, "free")
    );
    assert_eq!(used_by(&doc, "len"), vec!["twice".to_owned()]);
    // The extrusion's distance: the feature itself or its own parameter.
    let twice = used_by(&doc, "twice");
    assert!(!twice.is_empty(), "{:?}", parameter(&doc, "twice"));
    // Model parameters have no delete button: no `used_by`.
    let all: Value = serde_json::from_str(
        &doc.query(&json!({"query": "parameters"}).to_string())
            .unwrap(),
    )
    .unwrap();
    for p in all.as_array().unwrap() {
        if p["kind"] != "user" {
            assert!(p.get("used_by").is_none(), "{p}");
        }
    }
}

#[test]
fn delete_parameter_refuses_what_used_by_names() {
    let mut doc = design();
    for name in ["len", "twice"] {
        let refused = doc
            .command(&json!({"cmd": "delete_parameter", "name": name}).to_string())
            .map(|r| panic!("deleting {name} must be refused, got {r}"))
            .unwrap_err()
            .to_string();
        assert!(refused.contains("used by"), "{name}: {refused}");
        assert!(parameter(&doc, name).is_some());
    }
    command(&mut doc, json!({"cmd": "delete_parameter", "name": "free"}));
    assert!(parameter(&doc, "free").is_none());
    // One undo step brings it back, unused as before.
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(used_by(&doc, "free").is_empty());
}
