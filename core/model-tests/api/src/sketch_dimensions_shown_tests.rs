// SPDX-License-Identifier: MIT
//! Show Dimensions of a sketch's row (mitcad#1772, GitHub
//! mitcad/Mitcad#306): `set_sketch_dimensions_shown` keeps, as display
//! state, the sketches whose dimensions the view draws outside sketch mode:
//! an undo step that changes neither the sketch nor its light bulb and
//! recomputes nothing, saved with the design.

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

/// The `sketch` query's answer of a sketch, which the dimensions shown
/// leave as it is.
fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "sketch", "uid": uid}))
}

/// Part (C1) with Sketch1 (F1) and Cover (C2) with Sketch2 (F2), each a
/// rectangle, which `sketch.add_rectangle` dimensions (width and height).
fn two_components() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    for (name, sketch) in [("Part", "F1"), ("Cover", "F2")] {
        command(
            &mut doc,
            json!({"cmd": "activate_component", "component": "Root"}),
        );
        command(&mut doc, json!({"cmd": "create_component", "name": name}));
        command(&mut doc, json!({"cmd": "sketch.create"}));
        command(
            &mut doc,
            json!({"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": [0, 0],
                   "width": 30, "height": 20}),
        );
    }
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "Root"}),
    );
    doc
}

#[test]
fn show_dimensions_is_display_state_with_its_own_undo_step() {
    let mut doc = two_components();
    assert_eq!(display(&doc).get("sketch_dimensions"), None);
    let before = sketch(&doc, "F1");
    let timeline = query(&doc, json!({"query": "timeline"}));

    command(
        &mut doc,
        json!({"cmd": "set_sketch_dimensions_shown", "sketch": "F1", "shown": true}),
    );
    assert_eq!(undo_label(&doc), "Show Dimensions of Sketch1");
    assert_eq!(display(&doc)["sketch_dimensions"], json!(["F1"]));
    assert_eq!(sketch(&doc, "F1"), before, "the sketch stays as it was");
    assert_eq!(
        query(&doc, json!({"query": "timeline"})),
        timeline,
        "no feature, light bulb or marker changes"
    );

    // Several sketches in several components.
    command(
        &mut doc,
        json!({"cmd": "set_sketch_dimensions_shown", "sketch": "F2", "shown": true}),
    );
    assert_eq!(undo_label(&doc), "Show Dimensions of Sketch2");
    assert_eq!(display(&doc)["sketch_dimensions"], json!(["F1", "F2"]));

    // Nothing to change: no undo step.
    command(
        &mut doc,
        json!({"cmd": "set_sketch_dimensions_shown", "sketch": "F2", "shown": true}),
    );
    assert_eq!(undo_label(&doc), "Show Dimensions of Sketch2");

    command(
        &mut doc,
        json!({"cmd": "set_sketch_dimensions_shown", "sketch": "F1", "shown": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Dimensions of Sketch1");
    assert_eq!(display(&doc)["sketch_dimensions"], json!(["F2"]));

    // One step back at a time.
    doc.undo();
    assert_eq!(display(&doc)["sketch_dimensions"], json!(["F1", "F2"]));
    doc.undo();
    assert_eq!(display(&doc)["sketch_dimensions"], json!(["F1"]));
    doc.undo();
    assert_eq!(display(&doc).get("sketch_dimensions"), None);
    doc.redo();
    assert_eq!(display(&doc)["sketch_dimensions"], json!(["F1"]));
}

#[test]
fn show_dimensions_is_saved_with_the_design() {
    let mut doc = two_components();
    command(
        &mut doc,
        json!({"cmd": "set_sketch_dimensions_shown", "sketch": "F2", "shown": true}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["display"]["sketch_dimensions"], json!(["F2"]));
    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    opened.recompute();
    assert_eq!(display(&opened)["sketch_dimensions"], json!(["F2"]));

    // Hidden again: nothing of it is left in the file.
    command(
        &mut doc,
        json!({"cmd": "set_sketch_dimensions_shown", "sketch": "F2", "shown": false}),
    );
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file.get("display"), None);
}

#[test]
fn a_deleted_sketch_leaves_the_list() {
    let mut doc = two_components();
    command(
        &mut doc,
        json!({"cmd": "set_sketch_dimensions_shown", "sketch": "F1", "shown": true}),
    );
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F1"}));
    assert_eq!(display(&doc).get("sketch_dimensions"), None);
    doc.undo();
    assert_eq!(display(&doc)["sketch_dimensions"], json!(["F1"]));
}

#[test]
fn show_dimensions_needs_a_planar_sketch() {
    let mut doc = two_components();
    let refused = doc
        .command(
            &json!({"cmd": "set_sketch_dimensions_shown", "sketch": "F9", "shown": true})
                .to_string(),
        )
        .unwrap_err();
    assert!(refused.to_string().contains("does not exist"), "{refused}");

    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 10}}}),
    );
    let refused = doc
        .command(
            &json!({"cmd": "set_sketch_dimensions_shown", "sketch": "F3", "shown": true})
                .to_string(),
        )
        .unwrap_err();
    assert!(refused.to_string().contains("is not a sketch"), "{refused}");
    assert_eq!(display(&doc).get("sketch_dimensions"), None);
}
