// SPDX-License-Identifier: MIT
//! A pipe's Corners (mitcad#1448) with the mock kernel: a pipe without
//! `corners` (a design saved before the choice) reaches the kernel mitered
//! as before, is saved without it and reads back the same from the file;
//! `rounded` reaches the kernel and is kept; an unknown kind is refused;
//! the preview counts the path's sharp corners for the Pipe panel.

use serde_json::{Value, json};

use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn line(doc: &mut Document<MockKernel>, a: [f64; 2], b: [f64; 2]) -> String {
    let result = command(
        doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": a, "end": b}),
    );
    result["made"][0].as_str().unwrap().to_owned()
}

fn last_spec(doc: &Document<MockKernel>) -> String {
    doc.kernel()
        .specs
        .borrow()
        .last()
        .cloned()
        .unwrap_or_default()
}

fn pipe<S: AsRef<str>>(curves: &[S], extra: Value) -> Value {
    let curves: Vec<&str> = curves.iter().map(AsRef::<str>::as_ref).collect();
    let mut value = json!({"type": "pipe", "path": {"sketch": "F1", "curves": curves},
        "size": 8, "operation": "new_body"});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    value
}

/// The feature's definition as the model keeps it.
fn saved(doc: &Document<MockKernel>, uid: &str) -> Value {
    let answer: Value = serde_json::from_str(
        &doc.query(&json!({"query": "feature", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap();
    answer["def"].clone()
}

/// The pipe's definition in the design file.
fn in_file(doc: &Document<MockKernel>) -> Value {
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    file["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["type"] == "pipe")
        .cloned()
        .expect("a pipe in the file")
}

/// Sketch1 (F1) on XY with an L-shaped path: c1 from the origin along X to
/// (40, 0), then up to (40, 30). Its curve ids.
fn l_path() -> (Document<MockKernel>, String, String) {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "xy"}));
    let first = line(&mut doc, [0.0, 0.0], [40.0, 0.0]);
    let second = line(&mut doc, [40.0, 0.0], [40.0, 30.0]);
    (doc, first, second)
}

#[test]
fn a_pipe_saved_without_corners_stays_mitered() {
    let (mut doc, first, second) = l_path();
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": pipe(&[&first, &second], json!({}))}),
    );
    let uid = added["uid"].as_str().unwrap().to_owned();
    assert_eq!(doc.status(uid.parse().unwrap()), Some(&FeatureStatus::Ok));
    // The kernel's input as before the choice: no corners named.
    let mitered = last_spec(&doc);
    assert!(mitered.starts_with("pipe F2 path "), "{mitered}");
    assert!(!mitered.contains("corners"), "{mitered}");
    // Saved without the field, in the definition and in the file.
    assert!(
        saved(&doc, &uid).get("corners").is_none(),
        "{}",
        saved(&doc, &uid)
    );
    assert!(in_file(&doc).get("corners").is_none(), "{}", in_file(&doc));
    // Read back from the file: the same kernel input, saved the same.
    let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(
        reopened.status(uid.parse().unwrap()),
        Some(&FeatureStatus::Ok)
    );
    assert_eq!(last_spec(&reopened), mitered);
    assert!(in_file(&reopened).get("corners").is_none());
    // An explicit `mitered` is the same and is saved without the field.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid,
               "def": pipe(&[&first, &second], json!({"corners": "mitered"}))}),
    );
    assert_eq!(last_spec(&doc), mitered);
    assert!(saved(&doc, &uid).get("corners").is_none());
}

#[test]
fn rounded_corners_reach_the_kernel_and_are_kept() {
    let (mut doc, first, second) = l_path();
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature",
               "def": pipe(&[&first, &second], json!({"section": "square", "corners": "rounded"}))}),
    );
    let uid = added["uid"].as_str().unwrap().to_owned();
    assert_eq!(doc.status(uid.parse().unwrap()), Some(&FeatureStatus::Ok));
    let rounded = last_spec(&doc);
    assert!(rounded.ends_with(" corners Rounded"), "{rounded}");
    assert!(rounded.contains(" Square size 8 "), "{rounded}");
    assert_eq!(saved(&doc, &uid)["corners"], "rounded");
    assert_eq!(in_file(&doc)["corners"], "rounded");
    let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(last_spec(&reopened), rounded);
    assert_eq!(in_file(&reopened)["corners"], "rounded");
    // Only mitered and rounded: per-corner kinds are the sketch's fillets
    // and chamfers.
    let refused = doc.command(
        &json!({"cmd": "add_feature",
                "def": pipe(&[&first, &second], json!({"corners": "bevelled"}))})
        .to_string(),
    );
    assert!(refused.is_err(), "{refused:?}");
}

/// The preview's `sweep_path` of a new pipe.
fn preview_place(doc: &mut Document<MockKernel>, def: Value) -> Value {
    let command = json!({"cmd": "add_feature", "def": def});
    let report: Value = serde_json::from_str(&doc.preview(&command.to_string()).unwrap()).unwrap();
    report["sweep_path"].clone()
}

#[test]
fn pipe_previews_count_the_paths_corners() {
    let (mut doc, first, second) = l_path();
    // The L: one corner, the section at the open path's start.
    assert_eq!(
        preview_place(&mut doc, pipe(&[&first, &second], json!({}))),
        json!({"closed": false, "profile_at": "start", "corners": 1})
    );
    // Listed the other way round, the same path.
    assert_eq!(
        preview_place(&mut doc, pipe(&[&second, &first], json!({})))["corners"],
        1
    );
    // A straight path has none: the field is left out.
    let straight = line(&mut doc, [0.0, 100.0], [50.0, 100.0]);
    assert_eq!(
        preview_place(&mut doc, pipe(&[&straight], json!({}))),
        json!({"closed": false, "profile_at": "start"})
    );
    // A line, a quarter arc on from it and a line on from that: tangent
    // joints, no corners.
    let lead = line(&mut doc, [0.0, 200.0], [40.0, 200.0]);
    let arc = command(
        &mut doc,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": [40, 220],
               "start": [40, 200], "end": [60, 220]}),
    )["made"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let rise = line(&mut doc, [60.0, 220.0], [60.0, 260.0]);
    assert!(
        preview_place(&mut doc, pipe(&[&lead, &arc, &rise], json!({})))
            .get("corners")
            .is_none()
    );
    // A closed triangle: three corners, the last where it closes.
    let a = line(&mut doc, [0.0, 300.0], [40.0, 300.0]);
    let b = line(&mut doc, [40.0, 300.0], [40.0, 330.0]);
    let c = line(&mut doc, [40.0, 330.0], [0.0, 300.0]);
    assert_eq!(
        preview_place(&mut doc, pipe(&[&a, &b, &c], json!({}))),
        json!({"closed": true, "profile_at": "inside", "corners": 3})
    );
    // Nothing was added.
    assert_eq!(doc.features().count(), 1);
}
