// SPDX-License-Identifier: MIT
//! The order of occurrences among their siblings (mitcad#2104) with the
//! mock kernel: `reorder_occurrence` in the root and in a nested component,
//! several at once, its undo and redo, the design file, the refusals (an
//! index past the end, a parent linked from another file), and that it
//! recomputes nothing and changes no placement or timeline.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, InsertOptions};
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string()).unwrap_err().to_string()
}

/// A 10 x 10 x 5 block at `x` in the active component.
fn block(doc: &mut Document<MockKernel>, x: f64) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [x, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

/// Arm:1 (O1), Base:1 (O2) and Lid:1 (O3) in the root, each with a block,
/// placed apart; Arm holds Pin:1 (O4) and Clip:1 (O5).
fn assembly() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let mut arm = ComponentUid::ROOT;
    for (i, name) in ["Arm", "Base", "Lid"].iter().enumerate() {
        let (component, _) = doc
            .create_component(
                Some(name),
                Transform::translation([30.0 * i as f64, 0.0, 0.0]),
                true,
            )
            .unwrap();
        block(&mut doc, 0.0);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        if i == 0 {
            arm = component;
        }
    }
    doc.activate_component(arm).unwrap();
    for inner in ["Pin", "Clip"] {
        doc.create_component(Some(inner), Transform::IDENTITY, false)
            .unwrap();
    }
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc
}

/// The names of the occurrences the `components` query lists at `path`
/// (indices into the tree), in its order.
fn names_at(doc: &Document<MockKernel>, path: &[usize]) -> Vec<String> {
    let mut level = query(doc, json!({"query": "components"}))["occurrences"].clone();
    for i in path {
        level = level[*i]["children"].clone();
    }
    level
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap().to_owned())
        .collect()
}

/// What a reorder must leave as it was: every body's placement, the
/// timeline and the number of recomputes.
fn untouched(doc: &Document<MockKernel>) -> (Vec<Value>, Value, u64) {
    let mut placed: Vec<Value> = query(doc, json!({"query": "instances"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|i| json!([i["body"], i["occurrence"], i["transform"]]))
        .collect();
    placed.sort_by_key(Value::to_string);
    (
        placed,
        query(doc, json!({"query": "timeline"})),
        doc.recompute_count(),
    )
}

#[test]
fn occurrences_are_reordered_among_their_siblings_undone_and_saved() {
    let mut doc = assembly();
    let before = untouched(&doc);
    assert_eq!(names_at(&doc, &[]), ["Arm:1", "Base:1", "Lid:1"]);
    assert_eq!(names_at(&doc, &[0]), ["Pin:1", "Clip:1"]);

    // The last before the first, by name.
    let reply = command(
        &mut doc,
        json!({"cmd": "reorder_occurrence", "occurrence": "Lid:1", "index": 0}),
    );
    assert_eq!(reply["index"], 0);
    assert_eq!(names_at(&doc, &[]), ["Lid:1", "Arm:1", "Base:1"]);
    assert_eq!(untouched(&doc), before);

    // In a nested component, by path: Clip:1 first in Arm.
    command(
        &mut doc,
        json!({"cmd": "reorder_occurrence", "occurrence": "O1/O5", "index": 0}),
    );
    assert_eq!(names_at(&doc, &[1]), ["Clip:1", "Pin:1"]);
    assert_eq!(untouched(&doc), before);

    // The same place again is no undo step.
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "reorder_occurrence", "occurrence": "Lid:1", "index": 0}),
    );
    assert_eq!(doc.undo_depth(), depth);

    // Saved and opened again in the same order.
    let json = doc.to_json();
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    loaded.recompute();
    assert_eq!(names_at(&loaded, &[]), ["Lid:1", "Arm:1", "Base:1"]);
    assert_eq!(names_at(&loaded, &[1]), ["Clip:1", "Pin:1"]);

    // Undone and redone, one step each.
    assert_eq!(doc.undo().as_deref(), Some("Reorder Clip:1"));
    assert_eq!(names_at(&doc, &[1]), ["Pin:1", "Clip:1"]);
    assert_eq!(doc.undo().as_deref(), Some("Reorder Lid:1"));
    assert_eq!(names_at(&doc, &[]), ["Arm:1", "Base:1", "Lid:1"]);
    assert!(doc.redo().is_some());
    assert_eq!(names_at(&doc, &[]), ["Lid:1", "Arm:1", "Base:1"]);
    // The commands recomputed nothing; the two undos and the redo each
    // recompute the state they restore, as every undo does
    // (`Document::step`).
    assert_eq!(doc.recompute_count(), before.2 + 3);
}

#[test]
fn several_occurrences_move_together_in_the_order_given() {
    let mut doc = assembly();
    // Base:1 and Arm:1 (in that order) after Lid:1: index 1 among the one
    // left.
    command(
        &mut doc,
        json!({"cmd": "reorder_occurrence", "occurrence": "O2", "with": ["Arm:1"], "index": 1}),
    );
    assert_eq!(names_at(&doc, &[]), ["Lid:1", "Base:1", "Arm:1"]);
    assert_eq!(doc.undo().as_deref(), Some("Reorder 2 Components"));
    assert_eq!(names_at(&doc, &[]), ["Arm:1", "Base:1", "Lid:1"]);
}

#[test]
fn a_reorder_is_refused_past_the_end_across_parents_and_in_a_linked_component() {
    let mut doc = assembly();
    let error = rejected(
        &mut doc,
        json!({"cmd": "reorder_occurrence", "occurrence": "Lid:1", "index": 3}),
    );
    assert!(error.contains("out of range"), "{error}");
    let error = rejected(
        &mut doc,
        json!({"cmd": "reorder_occurrence", "occurrence": "Lid:1", "with": ["O1/O4"], "index": 0}),
    );
    assert!(
        error.contains("not placed in the same component"),
        "{error}"
    );

    // An occurrence placed in a linked component: its order is the file's.
    let dir = mitcad_model::testing::unique_temp_dir("reorder-linked");
    let mut part = Document::new(MockKernel::default());
    block(&mut part, 0.0);
    std::fs::write(dir.join("part.mitcad"), part.to_json()).unwrap();
    let options = InsertOptions {
        base: Some(dir.clone()),
        ..InsertOptions::default()
    };
    let (linked, _) = doc.insert_component("part.mitcad", &options).unwrap();
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    for o in file["occurrences"].as_array_mut().unwrap() {
        if o["uid"] == "O4" || o["uid"] == "O5" {
            o["parent"] = json!(linked);
        }
    }
    let mut inside = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    for refused in [
        json!({"cmd": "reorder_occurrence", "occurrence": "O5", "index": 0}),
        json!({"cmd": "set_occurrence_folder", "occurrence": "O5", "folder": "Clips"}),
    ] {
        let error = rejected(&mut inside, refused.clone());
        assert!(error.contains("linked from"), "{refused}: {error}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
