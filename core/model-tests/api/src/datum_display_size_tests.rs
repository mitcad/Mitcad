// SPDX-License-Identifier: MIT
//! How large construction planes and axes are drawn (mitcad#1748): the
//! `set_datum_display_size` command their resize handles send, the size in
//! the `datums` and `datum` queries, undo, the design file and deleting.

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

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .expect_err(&format!("{command} was accepted"))
        .0
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// Plane1 (F1) 10 mm above XY, Axis1 (F2) along Z, Point1 (F3) at the
/// origin.
fn design() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    for def in [
        json!({"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 10}}),
        json!({"type": "construction_axis",
               "definition": {"type": "two_planes", "plane1": "xz", "plane2": "yz"}}),
        json!({"type": "construction_point",
               "definition": {"type": "three_planes", "plane1": "xy", "plane2": "xz", "plane3": "yz"}}),
    ] {
        command(&mut doc, json!({"cmd": "add_feature", "def": def}));
    }
    doc
}

/// The `display_size` the `datums` query lists for a datum, null when none.
fn listed_size(doc: &Document<MockKernel>, uid: &str) -> Value {
    let datums = query(doc, json!({"query": "datums", "origin": true}));
    let datum = datums
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["uid"] == uid)
        .unwrap_or_else(|| panic!("{uid} is not in {datums}"))
        .clone();
    datum.get("display_size").cloned().unwrap_or(Value::Null)
}

#[test]
fn construction_planes_and_axes_keep_a_display_size_with_undo_and_in_the_file() {
    let mut doc = design();
    // None set: no size listed, the application draws its default.
    for uid in ["xy", "F1", "F2", "F3"] {
        assert_eq!(listed_size(&doc, uid), Value::Null, "{uid}");
    }

    let result = command(
        &mut doc,
        json!({"cmd": "set_datum_display_size", "uid": "F1", "size": 40}),
    );
    assert_eq!(result["recomputed"], 0, "{result}");
    assert_eq!(doc.undo_label(), Some("Resize Plane1"));
    command(
        &mut doc,
        json!({"cmd": "set_datum_display_size", "uid": "F2", "size": 250.5}),
    );
    assert_eq!(doc.undo_label(), Some("Resize Axis1"));
    assert_eq!(listed_size(&doc, "F1"), json!(40.0));
    assert_eq!(listed_size(&doc, "F2"), json!(250.5));
    assert_eq!(
        query(&doc, json!({"query": "datum", "uid": "F1"}))["display_size"],
        json!(40.0)
    );
    // The datum itself does not move or change.
    let plane = query(&doc, json!({"query": "datum", "uid": "F1"}));
    assert_eq!(plane["origin"], json!([0.0, 0.0, 10.0]), "{plane}");

    // The same size again is no undo step.
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "set_datum_display_size", "uid": "F1", "size": 40}),
    );
    assert_eq!(doc.undo_depth(), depth);

    // Undo and redo.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(listed_size(&doc, "F2"), Value::Null);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(listed_size(&doc, "F2"), json!(250.5));

    // Saved with the feature and read back.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["features"][0]["display_size"], json!(40.0));
    assert_eq!(file["features"][1]["display_size"], json!(250.5));
    assert!(file["features"][2].get("display_size").is_none());
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(listed_size(&loaded, "F1"), json!(40.0));
    assert_eq!(listed_size(&loaded, "F2"), json!(250.5));

    // Null goes back to the default size.
    command(
        &mut doc,
        json!({"cmd": "set_datum_display_size", "uid": "F1", "size": null}),
    );
    assert_eq!(listed_size(&doc, "F1"), Value::Null);
    assert_eq!(doc.undo_label(), Some("Resize Plane1"));

    // Deleting a feature forgets its size.
    command(
        &mut doc,
        json!({"cmd": "delete_feature", "uid": "F2", "dependents": true}),
    );
    assert!(!doc.to_json().contains("display_size"));
}

#[test]
fn only_construction_planes_and_axes_take_a_positive_display_size() {
    let mut doc = design();
    let error = refused(
        &mut doc,
        json!({"cmd": "set_datum_display_size", "uid": "F3", "size": 20}),
    );
    assert!(error.contains("Point1 has no display size"), "{error}");
    for size in [json!(0), json!(-5), json!(2.0e6)] {
        let error = refused(
            &mut doc,
            json!({"cmd": "set_datum_display_size", "uid": "F1", "size": size}),
        );
        assert!(error.contains("must be greater than 0"), "{size}: {error}");
    }
    // A file with a size on another feature, or one that is no size.
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    file["features"][2]["display_size"] = json!(30);
    let loaded = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    assert_eq!(loaded.datum_display_size("F3".parse().unwrap()), None);
    file["features"][0]["display_size"] = json!(-1);
    let error = Document::from_json(&file.to_string(), MockKernel::default())
        .err()
        .expect("a negative display size was read");
    assert!(error.to_string().contains("display_size"), "{error}");
}
