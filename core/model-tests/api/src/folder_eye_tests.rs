// SPDX-License-Identifier: MIT
//! The eyes of each component's Bodies, Sketches and Construction folders
//! (mitcad#1771, GitHub mitcad/Mitcad#292): `set_folder_visible` hides and
//! shows a folder as display state, one undo step that changes no light
//! bulb; a shown folder lets each item follow its own light bulb again, and
//! the design keeps the folder's eye when it is saved and opened.

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

/// The sketches' light bulbs: (uid, visible) of each sketch, in timeline
/// order.
fn sketch_bulbs(doc: &Document<MockKernel>) -> Vec<(String, bool)> {
    query(doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["type"] == "sketch")
        .map(|f| {
            (
                f["uid"].as_str().unwrap().to_owned(),
                f["visible"].as_bool().unwrap(),
            )
        })
        .collect()
}

/// The bodies' own light bulbs: (uid, visible), a shown body having no
/// `visible` in the `bodies` query.
fn body_bulbs(doc: &Document<MockKernel>) -> Vec<(String, bool)> {
    query(doc, json!({"query": "bodies"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["uid"].as_str().unwrap().to_owned(),
                b.get("visible").and_then(Value::as_bool).unwrap_or(true),
            )
        })
        .collect()
}

/// The bodies the view draws: the `instances` query's (only the visible
/// ones), by body uid.
fn drawn_bodies(doc: &Document<MockKernel>) -> Vec<String> {
    query(doc, json!({"query": "instances"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["body"].as_str().unwrap().to_owned())
        .collect()
}

/// Saved and opened again.
fn reopened(doc: &Document<MockKernel>) -> Document<MockKernel> {
    let mut opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    opened.recompute();
    opened
}

/// Five sketches (F1-F5) on XY, none used by a feature, so all shown.
fn five_sketches() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    for i in 0..5 {
        command(&mut doc, json!({"cmd": "sketch.create"}));
        command(
            &mut doc,
            json!({"cmd": "sketch.add_rectangle", "sketch": format!("F{}", i + 1),
                   "corner": [i * 50, 0], "width": 40, "height": 20}),
        );
    }
    doc
}

/// Five boxes (F1-F5, their bodies F1.b0-F5.b0) side by side.
fn five_boxes() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    for i in 0..5 {
        command(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy",
                   "corner": [i * 30, 0], "length": 20, "width": 20, "height": 10,
                   "operation": "new_body"}}),
        );
    }
    doc
}

#[test]
fn the_sketches_folder_keeps_the_sketches_own_eyes() {
    let mut doc = five_sketches();
    // Two of the five hidden by their own light bulbs.
    for uid in ["F2", "F4"] {
        command(
            &mut doc,
            json!({"cmd": "set_feature_visible", "uid": uid, "visible": false}),
        );
    }
    let bulbs = sketch_bulbs(&doc);
    let expected: Vec<(String, bool)> = ["F1", "F2", "F3", "F4", "F5"]
        .iter()
        .map(|uid| (uid.to_string(), *uid != "F2" && *uid != "F4"))
        .collect();
    assert_eq!(bulbs, expected);
    assert_eq!(display(&doc).get("folders"), None);

    // The folder hidden: one undo step, no light bulb changes.
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "component": "C0", "folder": "sketches", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Sketches");
    assert_eq!(
        display(&doc)["folders"],
        json!({"C0": {"bodies": true, "sketches": false, "construction": true}})
    );
    assert_eq!(sketch_bulbs(&doc), bulbs, "no light bulb changes");

    // Hidden again: nothing changes, no undo step.
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "folder": "sketches", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Sketches");

    // Saved hidden and opened: still hidden, the light bulbs as they were.
    let mut opened = reopened(&doc);
    assert_eq!(display(&opened), display(&doc));
    assert_eq!(sketch_bulbs(&opened), bulbs);

    // Shown again: each sketch follows its own light bulb, the same two
    // hidden; nothing of the folder is left in the display state.
    command(
        &mut opened,
        json!({"cmd": "set_folder_visible", "component": "Root", "folder": "sketches", "visible": true}),
    );
    assert_eq!(undo_label(&opened), "Show Sketches");
    assert_eq!(display(&opened).get("folders"), None);
    assert_eq!(sketch_bulbs(&opened), bulbs);

    // Hidden and shown, then saved and opened: the same two hidden.
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "folder": "sketches", "visible": true}),
    );
    let again = reopened(&doc);
    assert_eq!(sketch_bulbs(&again), bulbs);
    assert_eq!(display(&again).get("folders"), None);
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file.get("display"), None, "nothing left in the file");

    // Undo takes the folder's steps back one at a time.
    doc.undo();
    assert_eq!(
        display(&doc)["folders"]["C0"]["sketches"],
        json!(false),
        "undo hides the folder again"
    );
    doc.undo();
    assert_eq!(display(&doc).get("folders"), None);
    assert_eq!(sketch_bulbs(&doc), bulbs);
}

#[test]
fn the_bodies_folder_keeps_the_bodies_own_eyes() {
    let mut doc = five_boxes();
    for uid in ["F2.b0", "F4.b0"] {
        command(
            &mut doc,
            json!({"cmd": "set_body_visible", "uid": uid, "visible": false}),
        );
    }
    let bulbs = body_bulbs(&doc);
    assert_eq!(
        drawn_bodies(&doc),
        ["F1.b0", "F3.b0", "F5.b0"],
        "two of five hidden by their own light bulbs"
    );

    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "folder": "bodies", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Bodies");
    assert!(drawn_bodies(&doc).is_empty(), "the folder hides every body");
    assert_eq!(body_bulbs(&doc), bulbs, "no light bulb changes");

    // Saved hidden: the file keeps the folder's eye; opened, nothing drawn.
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(file["display"]["folders"], json!({"C0": {"bodies": true}}));
    let mut opened = reopened(&doc);
    assert!(drawn_bodies(&opened).is_empty());
    assert_eq!(body_bulbs(&opened), bulbs);

    command(
        &mut opened,
        json!({"cmd": "set_folder_visible", "folder": "bodies", "visible": true}),
    );
    assert_eq!(undo_label(&opened), "Show Bodies");
    assert_eq!(drawn_bodies(&opened), ["F1.b0", "F3.b0", "F5.b0"]);
    assert_eq!(body_bulbs(&opened), bulbs);

    // Hidden and shown, saved and opened: the same two hidden.
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "folder": "bodies", "visible": true}),
    );
    let again = reopened(&doc);
    assert_eq!(drawn_bodies(&again), ["F1.b0", "F3.b0", "F5.b0"]);
    assert_eq!(body_bulbs(&again), bulbs);
}

#[test]
fn a_light_bulb_changes_while_its_folder_is_hidden() {
    let mut doc = five_boxes();
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "folder": "bodies", "visible": false}),
    );
    // A body's own light bulb still changes; it takes effect when the
    // folder is shown.
    command(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F1.b0", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Body1");
    assert!(drawn_bodies(&doc).is_empty());
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "folder": "bodies", "visible": true}),
    );
    assert_eq!(drawn_bodies(&doc), ["F2.b0", "F3.b0", "F4.b0", "F5.b0"]);
}

#[test]
fn show_when_keeps_working_under_the_bodies_folder() {
    let mut doc = five_boxes();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Holes", "expression": "'No'"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "expression": "Holes == 'Yes'"}),
    );
    assert!(!drawn_bodies(&doc).contains(&"F2.b0".to_owned()));
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "folder": "bodies", "visible": false}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Holes", "expression": "'Yes'"}),
    );
    assert!(drawn_bodies(&doc).is_empty(), "the folder hides it");
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "folder": "bodies", "visible": true}),
    );
    assert!(
        drawn_bodies(&doc).contains(&"F2.b0".to_owned()),
        "shown by its condition once the folder is"
    );
    let body = query(&doc, json!({"query": "bodies"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == "F2.b0")
        .cloned()
        .unwrap();
    assert_eq!(
        body["visible_when"], "Holes == 'Yes'",
        "the condition stays"
    );
}

#[test]
fn each_component_has_folders_of_its_own() {
    let mut doc = five_boxes();
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Part", "activate": true}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy",
               "corner": [0, 50], "length": 20, "width": 20, "height": 10,
               "operation": "new_body"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "component": "Part", "folder": "bodies", "visible": false}),
    );
    assert_eq!(
        display(&doc)["folders"],
        json!({"C1": {"bodies": false, "sketches": true, "construction": true}})
    );
    assert_eq!(
        drawn_bodies(&doc),
        ["F1.b0", "F2.b0", "F3.b0", "F4.b0", "F5.b0"],
        "the root's bodies stay; the component's are hidden"
    );
    // Another folder of the same component, and the root's construction.
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "component": "C1", "folder": "construction", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Construction");
    command(
        &mut doc,
        json!({"cmd": "set_folder_visible", "component": "C1", "folder": "bodies", "visible": true}),
    );
    assert_eq!(
        display(&doc)["folders"],
        json!({"C1": {"bodies": true, "sketches": true, "construction": false}})
    );
    // A component that is not there is refused.
    assert!(
        doc.command(
            &json!({"cmd": "set_folder_visible", "component": "C9", "folder": "bodies", "visible": false})
                .to_string()
        )
        .is_err()
    );
}
