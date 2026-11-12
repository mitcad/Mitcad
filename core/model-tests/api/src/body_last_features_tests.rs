// SPDX-License-Identifier: MIT
//! The `bodies` query's `last_features` (mitcad#1704): each body's last
//! feature before the marker, which the timeline marks while the body or a
//! face, edge or vertex of it is selected in the view.

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
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// Each body's uid and `last_feature`, in the query's order.
fn last_features(doc: &Document<MockKernel>) -> Vec<(Value, Value)> {
    query(doc, json!({"query": "bodies", "last_features": true}))
        .as_array()
        .unwrap()
        .iter()
        .map(|body| (body["uid"].clone(), body["last_feature"].clone()))
        .collect()
}

fn add_box(doc: &mut Document<MockKernel>, corner: [f64; 2]) -> String {
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": corner,
               "length": 30, "width": 20, "height": 10, "operation": "new_body"}}),
    );
    added["uid"].as_str().unwrap().to_owned()
}

#[test]
fn a_body_s_last_feature_is_the_last_one_before_the_marker_that_set_it() {
    let mut doc = Document::new(MockKernel::default());
    let first = add_box(&mut doc, [0.0, 0.0]);
    assert_eq!(
        last_features(&doc),
        vec![(json!(format!("{first}.b0")), json!(first))]
    );
    // Not asked for: no such field.
    let plain = query(&doc, json!({"query": "bodies"}));
    assert!(plain[0].get("last_feature").is_none(), "{plain}");

    // A move of the body is its last feature; another body keeps its own.
    let moved = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "move", "bodies": [format!("{first}.b0")],
               "transform": {"type": "translate_xyz", "x": 0, "y": 0, "z": 5}}}),
    );
    let moved = moved["uid"].as_str().unwrap().to_owned();
    let second = add_box(&mut doc, [50.0, 0.0]);
    assert_eq!(
        last_features(&doc),
        vec![
            (json!(format!("{first}.b0")), json!(moved)),
            (json!(format!("{second}.b0")), json!(second)),
        ]
    );

    // With the marker before the move: the box again, and the second box's
    // body is not there.
    command(&mut doc, json!({"cmd": "set_marker", "position": 1}));
    assert_eq!(
        last_features(&doc),
        vec![(json!(format!("{first}.b0")), json!(first))]
    );
}
