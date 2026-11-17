// SPDX-License-Identifier: MIT
//! The direction of gravity (mitcad#1713, Part B) through the JSON api,
//! with the mock kernel: set, kept by the file and cleared; a Flex item
//! with a fixed length hanging its slack toward gravity onto a plate, and
//! warning when it is too short. (A Flex item let hang interactively:
//! `flex_hang_tests.rs`.)

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, value: Value) -> String {
    doc.command(&value.to_string()).unwrap_err().to_string()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn add_box(doc: &mut Document<MockKernel>, z: f64, corner: [f64; 2], size: [f64; 3]) -> Value {
    command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "box",
               "plane": {"origin": [0, 0, z], "normal": [0, 0, 1]}, "corner": corner,
               "length": size[0], "width": size[1], "height": size[2],
               "operation": "new_body"}}),
    )
}

/// A floor (F1, z -10 to 0), a step 50 high (F2) and a box on it (F3).
fn step() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    add_box(&mut doc, -10.0, [-150.0, -100.0], [400.0, 200.0, 10.0]);
    add_box(&mut doc, 0.0, [-100.0, -50.0], [100.0, 100.0, 50.0]);
    add_box(&mut doc, 50.0, [-4.0, -5.0], [10.0, 10.0, 30.0]);
    doc
}

#[test]
fn the_direction_of_gravity_is_set_kept_by_the_file_and_cleared() {
    let mut doc = step();
    assert_eq!(
        query(&doc, json!({"query": "gravity"}))["direction"],
        Value::Null
    );
    // Against a plane's normal, along an axis, the other way.
    let set = command(&mut doc, json!({"cmd": "gravity.set", "geometry": "xy"}));
    assert_eq!(set["direction"], json!([0.0, 0.0, -1.0]), "{set}");
    assert_eq!(set["source"]["geometry"], "xy");
    let set = command(&mut doc, json!({"cmd": "gravity.set", "geometry": "x"}));
    assert_eq!(set["direction"], json!([1.0, 0.0, 0.0]), "{set}");
    let set = command(
        &mut doc,
        json!({"cmd": "gravity.set", "geometry": "xz", "reverse": true}),
    );
    assert_eq!(set["direction"], json!([0.0, 1.0, 0.0]), "{set}");
    let set = command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -2]}),
    );
    assert_eq!(set["direction"], json!([0.0, 0.0, -1.0]));
    assert_eq!(set["source"], Value::Null);
    // Kept by the file.
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file["gravity"], json!({"direction": [0.0, 0.0, -1.0]}));
    let reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert_eq!(
        query(&reopened, json!({"query": "gravity"}))["direction"],
        json!([0.0, 0.0, -1.0])
    );
    // Each set an undo step; cleared.
    doc.undo().unwrap();
    assert_eq!(
        query(&doc, json!({"query": "gravity"}))["direction"],
        json!([0.0, 1.0, 0.0])
    );
    let cleared = command(&mut doc, json!({"cmd": "gravity.clear"}));
    assert_eq!(cleared["direction"], Value::Null);
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(file.get("gravity").is_none(), "{file}");
    // Refused: the origin point, a zero vector, both or neither.
    for wrong in [
        json!({"cmd": "gravity.set", "geometry": "origin"}),
        json!({"cmd": "gravity.set", "direction": [0, 0, 0]}),
        json!({"cmd": "gravity.set"}),
    ] {
        let error = refused(&mut doc, wrong.clone());
        assert!(!error.is_empty(), "{wrong}");
    }
}

fn hose() -> Value {
    json!({"name": "Hose 10", "section": {"type": "round", "diameter": 10}, "bend_radius": 30})
}

fn lowest_and_length(doc: &Document<MockKernel>, uid: &str) -> (f64, f64, Value) {
    let answer = query(doc, json!({"query": "flex", "uid": uid}));
    let lowest = answer["polyline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p[2].as_f64().unwrap())
        .fold(f64::INFINITY, f64::min);
    (lowest, answer["length"].as_f64().unwrap(), answer)
}

#[test]
fn a_flex_item_with_a_fixed_length_hangs_toward_gravity_onto_a_plate() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    // A plate 5 thick under a hose stretched 100 long at 50 high.
    add_box(&mut doc, 0.0, [0.0, -20.0], [100.0, 40.0, 5.0]);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "flex", "item": hose(),
               "fixed_length": 130, "nodes": [
            {"id": "n1", "at": [0, 0, 50]}, {"id": "n2", "at": [100, 0, 50]}]}}),
    );
    // Without gravity the slack is a number only.
    let (lowest, length, answer) = lowest_and_length(&doc, "F2");
    assert!((length - 100.0).abs() < 1e-9 && lowest == 50.0, "{answer}");
    assert!(
        (answer["slack"].as_f64().unwrap() - 30.0).abs() < 1e-9,
        "{answer}"
    );
    // With it, 130 hangs about 40 low, clear of the plate, about as long
    // as the fixed length.
    command(
        &mut doc,
        json!({"cmd": "gravity.set", "direction": [0, 0, -1]}),
    );
    let (lowest, length, answer) = lowest_and_length(&doc, "F2");
    assert!(lowest < 25.0 && lowest > 10.0, "hangs: {lowest} {answer}");
    assert!(
        (length - 130.0).abs() < 1.3,
        "its length: {length} {answer}"
    );
    assert_eq!(answer["problems"], json!([]), "{answer}");
    // Its ends stay.
    assert_eq!(answer["nodes"][0]["point"], json!([0.0, 0.0, 50.0]));
    assert_eq!(answer["nodes"][1]["point"], json!([100.0, 0.0, 50.0]));
    assert_eq!(answer["nodes"][1]["along"], answer["length"]);
    // 200 would hang below the plate's top: it rests on it, the hose's
    // radius above it.
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "fixed_length": 200}),
    );
    let (lowest, length, answer) = lowest_and_length(&doc, "F2");
    assert!(
        (lowest - 10.0).abs() < 1e-6,
        "rests on the plate: {lowest} {answer}"
    );
    assert!(length > 130.0 && length < 200.0, "{length}");
    // Shorter than the route: a warning in words, at the ends.
    command(
        &mut doc,
        json!({"cmd": "flex.set_item", "flex": "F2", "fixed_length": 80}),
    );
    let (_, length, answer) = lowest_and_length(&doc, "F2");
    assert!((length - 100.0).abs() < 1e-9, "{answer}");
    assert_eq!(answer["problems"][0]["kind"], "too_short", "{answer}");
    assert_eq!(
        answer["problems"][0]["message"],
        "the item's fixed length is too short to reach between its ends"
    );
    assert_eq!(
        answer["problems"][0]["places"].as_array().map(Vec::len),
        Some(2)
    );
}
