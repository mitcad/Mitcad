// SPDX-License-Identifier: MIT
//! View › Show of the whole design (mitcad#1287): `set_view_show` hides
//! and shows the origins, the construction geometry and the sketches as
//! display state, an undo step that changes no light bulb, saved with the
//! design.

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

fn display(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["display"].clone()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

/// The `timeline` query's `visible` of each feature with a light bulb.
fn light_bulbs(doc: &Document<MockKernel>) -> Vec<(Value, Value)> {
    query(doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f.get("visible").map(|v| (f["uid"].clone(), v.clone())))
        .collect()
}

/// Sketch1 (F1) with a rectangle, Plane1 (F2) 10 mm above XY and Sketch2
/// (F3) on it, none used by a feature.
fn part() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 10}}}),
    );
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "F2"}));
    doc
}

#[test]
fn view_show_hides_kinds_without_changing_light_bulbs() {
    let mut doc = part();
    let bulbs = light_bulbs(&doc);
    assert!(
        !bulbs.is_empty(),
        "the sketches and the plane have light bulbs"
    );
    assert_eq!(display(&doc).get("show"), None);

    command(
        &mut doc,
        json!({"cmd": "set_view_show", "construction": false}),
    );
    assert_eq!(undo_label(&doc), "Hide All Construction");
    assert_eq!(
        display(&doc)["show"],
        json!({"origins": true, "construction": false, "sketches": true})
    );
    assert_eq!(light_bulbs(&doc), bulbs, "no light bulb changes");

    command(
        &mut doc,
        json!({"cmd": "set_view_show", "origins": false, "sketches": false, "construction": false}),
    );
    assert_eq!(undo_label(&doc), "Hide All Origins, Hide All Sketches");
    assert_eq!(
        display(&doc)["show"],
        json!({"origins": false, "construction": false, "sketches": false})
    );
    assert_eq!(light_bulbs(&doc), bulbs);

    // Nothing to change: no undo step.
    command(&mut doc, json!({"cmd": "set_view_show", "sketches": false}));
    assert_eq!(undo_label(&doc), "Hide All Origins, Hide All Sketches");

    // Undo takes the steps back one at a time.
    doc.undo();
    assert_eq!(
        display(&doc)["show"],
        json!({"origins": true, "construction": false, "sketches": true})
    );
    doc.undo();
    assert_eq!(display(&doc).get("show"), None);
    doc.redo();
    command(
        &mut doc,
        json!({"cmd": "set_view_show", "construction": true}),
    );
    assert_eq!(undo_label(&doc), "Show All Construction");
    assert_eq!(display(&doc).get("show"), None);
    assert_eq!(light_bulbs(&doc), bulbs);
}

#[test]
fn view_show_is_saved_with_the_design() {
    let mut doc = part();
    command(
        &mut doc,
        json!({"cmd": "set_view_show", "construction": false, "sketches": false}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["display"]["hidden"],
        json!({"construction": true, "sketches": true})
    );
    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    opened.recompute();
    assert_eq!(display(&opened), display(&doc));

    // Shown again: nothing of it is left in the file.
    command(
        &mut doc,
        json!({"cmd": "set_view_show", "construction": true, "sketches": true}),
    );
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file.get("display"), None);
}

#[test]
fn view_show_needs_a_kind() {
    let mut doc = part();
    let refused = doc
        .command(&json!({"cmd": "set_view_show"}).to_string())
        .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("origins, construction or sketches"),
        "{refused}"
    );
}
