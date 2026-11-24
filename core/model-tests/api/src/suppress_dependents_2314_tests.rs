// SPDX-License-Identifier: MIT
//! `suppress_feature` with `dependents` (mitcad#2314): a feature and the
//! active features that use it are suppressed in one undo step; a feature
//! whose suppression follows a condition is left to it; without the option
//! the command does what it did before.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::FeatureUid;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const EDGE: &str = "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .expect_err("refused")
        .to_string()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn undo_depth(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo_depth"].clone()
}

/// The `suppressed` flags of the timeline's features, in order.
fn suppressed(doc: &Document<MockKernel>) -> Vec<bool> {
    query(doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["suppressed"].as_bool().unwrap())
        .collect()
}

/// Sketch1 (F1), Extrude1 (F2, the block F2.b0), Sketch2 (F3, a circle),
/// Extrude2 (F4, a hole cut through the block) and Fillet1 (F5, an edge
/// of the block rounded): Extrude → Cut → Fillet.
fn chain() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": REGION}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 20], "diameter": 10}),
    );
    let cut = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": "r{c1}"}],
               "extent": {"type": "through_all"}, "operation": "cut",
               "participants": ["F2.b0"]}}),
    );
    assert_eq!(cut["uid"], "F4");
    let fillet = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
               "edges": [EDGE], "radius": 5}}),
    );
    assert_eq!(fillet["uid"], "F5");
    assert_eq!(
        query(&doc, json!({"query": "dependents", "uid": "F2"})),
        json!({"dependents": [{"uid": "F4", "name": "Extrude2"},
                              {"uid": "F5", "name": "Fillet1"}]})
    );
    doc
}

#[test]
fn a_chain_is_suppressed_in_one_undo_step() {
    let mut doc = chain();
    let depth = undo_depth(&doc);
    let result = command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F2", "dependents": true}),
    );
    assert_eq!(result["suppressed"], json!(["F2", "F4", "F5"]), "{result}");
    // Nothing fails: the features that used the block are suppressed too.
    assert!(result.get("failed_features").is_none(), "{result}");
    assert_eq!(result["error"], Value::Null);
    assert_eq!(suppressed(&doc), [false, true, false, true, true]);
    assert_eq!(
        undo_label(&doc),
        "Suppress Extrude1 and the features that use it"
    );
    assert_eq!(undo_depth(&doc), json!(depth.as_u64().unwrap() + 1));

    // One undo brings the whole chain back.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(suppressed(&doc), [false; 5]);
    assert_eq!(undo_depth(&doc), depth);
    assert_eq!(query(&doc, json!({"query": "bodies"}))[0]["uid"], "F2.b0");

    // A dependent suppressed already stays so and is not listed; the
    // feature in the middle of the chain takes only what uses it.
    command(&mut doc, json!({"cmd": "suppress_feature", "uid": "F5"}));
    let result = command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F2", "dependents": true}),
    );
    assert_eq!(result["suppressed"], json!(["F2", "F4"]));
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));

    // A feature with no active dependents: its own step, by its own name.
    let result = command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F4", "dependents": true}),
    );
    assert_eq!(result["suppressed"], json!(["F4"]));
    assert_eq!(undo_label(&doc), "Suppress Extrude2");

    // Nothing left to change: an empty list and no undo step.
    let depth = undo_depth(&doc);
    let result = command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F4", "dependents": true}),
    );
    assert_eq!(result["suppressed"], json!([]));
    assert_eq!(undo_depth(&doc), depth);
}

#[test]
fn a_dependent_bound_to_a_condition_is_left_to_it() {
    let mut doc = chain();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Rounded", "expression": "'Yes'"}),
    );
    // The fillet is suppressed while the condition holds; now it does not.
    command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F5", "expression": "Rounded == 'No'"}),
    );
    assert_eq!(suppressed(&doc), [false; 5]);

    let result = command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F2", "dependents": true}),
    );
    assert_eq!(result["suppressed"], json!(["F2", "F4"]));
    assert_eq!(suppressed(&doc), [false, true, false, true, false]);
    assert_eq!(doc.suppressed_when(FeatureUid(5)), Some("Rounded == 'No'"));
    // It still follows its condition.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Rounded", "expression": "'No'"}),
    );
    assert_eq!(suppressed(&doc), [false, true, false, true, true]);
}

#[test]
fn without_dependents_only_the_feature_changes_as_before() {
    let mut doc = chain();
    for off in [
        json!({"cmd": "suppress_feature", "uid": "F2"}),
        json!({"cmd": "suppress_feature", "uid": "F2", "dependents": false}),
    ] {
        let result = command(&mut doc, off.clone());
        assert!(result.get("suppressed").is_none(), "{off}: {result}");
        assert_eq!(undo_label(&doc), "Suppress Extrude1");
        // The features that use the block stay active, and fail.
        assert_eq!(suppressed(&doc), [false, true, false, false, false]);
        let failed: Vec<&str> = result["failed_features"]
            .as_array()
            .map(|f| f.iter().filter_map(|f| f["uid"].as_str()).collect())
            .unwrap_or_default();
        assert!(failed.contains(&"F5"), "{off}: {result}");
        command(&mut doc, json!({"cmd": "undo"}));
        assert_eq!(suppressed(&doc), [false; 5]);
    }

    // Unsuppressing takes no dependents, and neither does a condition.
    command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F2", "dependents": true}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F2", "suppressed": false,
               "dependents": true}),
    );
    assert!(error.contains("`dependents`"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F2", "expression": "1",
               "dependents": true}),
    );
    assert!(error.contains("`dependents`"), "{error}");
    // Unsuppressed by hand, the feature alone comes back.
    command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F2", "suppressed": false}),
    );
    assert_eq!(suppressed(&doc), [false, false, false, true, true]);
}
