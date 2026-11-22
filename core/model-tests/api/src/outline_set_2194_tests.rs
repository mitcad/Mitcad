// SPDX-License-Identifier: MIT
//! The outline set in the `sketch` and `profiles` queries (mitcad#2194): a
//! plate with a hole gives the plate's ring, the hole's disc is marked as
//! outside it; two overlapping circles give none and say that the sketch's
//! closed curves cross.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    let result = doc
        .command(&value.to_string())
        .unwrap_or_else(|error| panic!("{value}: {error}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn sketch(entities: Value) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": entities}}),
    );
    doc
}

#[test]
fn a_plate_with_a_hole_gives_its_ring_and_marks_the_disc() {
    let doc = sketch(json!([
        {"id": "p1", "type": "point", "at": [0, 0]},
        {"id": "p2", "type": "point", "at": [100, 0]},
        {"id": "p3", "type": "point", "at": [100, 60]},
        {"id": "p4", "type": "point", "at": [0, 60]},
        {"id": "c5", "type": "line", "start": "p1", "end": "p2"},
        {"id": "c6", "type": "line", "start": "p2", "end": "p3"},
        {"id": "c7", "type": "line", "start": "p3", "end": "p4"},
        {"id": "c8", "type": "line", "start": "p4", "end": "p1"},
        {"id": "p9", "type": "point", "at": [30, 30]},
        {"id": "c10", "type": "circle", "center": "p9", "radius": 10}]));
    let answer = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    assert_eq!(answer["curves_cross"], false, "{answer}");
    let set = answer["outline_set"].as_array().unwrap();
    assert_eq!(set.len(), 1, "{answer}");
    let ring = set[0].as_str().unwrap();
    assert_ne!(ring, "r{c10}");
    let profiles = query(&doc, json!({"query": "profiles", "sketch": "F1"}));
    let rows = profiles.as_array().unwrap();
    assert_eq!(rows.len(), 2, "{profiles}");
    for row in rows {
        let member = row["region"] == json!(ring);
        assert_eq!(row["outline"], json!(member), "{row}");
        assert!(row.get("curves_cross").is_none(), "{row}");
    }
}

#[test]
fn two_overlapping_circles_give_no_outline_set() {
    let doc = sketch(json!([
        {"id": "p1", "type": "point", "at": [0, 0]},
        {"id": "c2", "type": "circle", "center": "p1", "radius": 10},
        {"id": "p3", "type": "point", "at": [12, 0]},
        {"id": "c4", "type": "circle", "center": "p3", "radius": 10}]));
    let answer = query(&doc, json!({"query": "sketch", "uid": "F1"}));
    assert_eq!(answer["curves_cross"], true, "{answer}");
    assert_eq!(answer["outline_set"], json!([]), "{answer}");
    let profiles = query(&doc, json!({"query": "profiles", "sketch": "F1"}));
    let rows = profiles.as_array().unwrap();
    assert_eq!(rows.len(), 3, "{profiles}");
    for row in rows {
        assert_eq!(row["curves_cross"], true, "{row}");
        assert!(row.get("outline").is_none(), "{row}");
    }
}
