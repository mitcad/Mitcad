// SPDX-License-Identifier: MIT
//! Folders of a component's occurrences and the rename of a Bodies folder's
//! folder (mitcad#2103) with the mock kernel: the commands and their undo
//! steps, what the `components` query lists, the design file, a reparented
//! occurrence leaving its folder, and that no folder command recomputes or
//! changes the timeline or the bill of materials.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
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

/// A 10 x 10 x 5 block (Sketch and Extrude) at `x` in the active component.
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

/// Rail:1 (O1), Post:1 (O2) and Panel:1 (O3) in the root, each with a block.
fn three_components() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true); // the bill of materials measures
    for (i, name) in ["Rail", "Post", "Panel"].iter().enumerate() {
        doc.create_component(Some(name), Transform::IDENTITY, true)
            .unwrap();
        block(&mut doc, 20.0 * i as f64);
        doc.activate_component(ComponentUid::ROOT).unwrap();
    }
    doc
}

/// The root's occurrences in the `components` query: name and folder
/// (empty when in none), in the query's order.
fn listed(doc: &Document<MockKernel>) -> Vec<(String, String)> {
    query(doc, json!({"query": "components"}))["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["name"].as_str().unwrap().to_owned(),
                o["folder"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn pairs(rows: &[(&str, &str)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

/// What a folder command must leave as it was: the timeline, the bill of
/// materials and the number of recomputes.
fn untouched(doc: &Document<MockKernel>) -> (Value, Value, u64) {
    (
        query(doc, json!({"query": "timeline"})),
        query(doc, json!({"query": "parts_list"})),
        doc.recompute_count(),
    )
}

#[test]
fn an_occurrence_folder_is_set_renamed_cleared_saved_and_undone() {
    let mut doc = three_components();
    let before = untouched(&doc);
    assert_eq!(
        listed(&doc),
        pairs(&[("Rail:1", ""), ("Post:1", ""), ("Panel:1", "")])
    );

    // Two occurrences into Frame, by name and by uid; the same again is no
    // undo step.
    let reply = command(
        &mut doc,
        json!({"cmd": "set_occurrence_folder", "occurrence": "Rail:1", "folder": " Frame "}),
    );
    assert_eq!(reply["folder"], "Frame");
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_folder", "occurrence": "O3", "folder": "Frame"}),
    );
    let depth = query(&doc, json!({"query": "document"}))["undo"].clone();
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_folder", "occurrence": "O3", "folder": "Frame"}),
    );
    assert_eq!(query(&doc, json!({"query": "document"}))["undo"], depth);
    assert_eq!(
        listed(&doc),
        pairs(&[("Rail:1", "Frame"), ("Post:1", ""), ("Panel:1", "Frame")])
    );
    assert_eq!(doc.occurrence_folders(ComponentUid::ROOT), vec!["Frame"]);
    assert_eq!(untouched(&doc), before);

    // Renamed: both occurrences take the name.
    command(
        &mut doc,
        json!({"cmd": "rename_occurrence_folder", "component": "C0", "folder": "Frame", "name": "Profiles"}),
    );
    assert_eq!(
        listed(&doc),
        pairs(&[
            ("Rail:1", "Profiles"),
            ("Post:1", ""),
            ("Panel:1", "Profiles")
        ])
    );
    assert_eq!(untouched(&doc), before);

    // Saved with the occurrences and read back.
    let json = doc.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    let folders: Vec<Value> = file["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o.get("folder").cloned().unwrap_or(Value::Null))
        .collect();
    assert_eq!(
        folders,
        vec![json!("Profiles"), Value::Null, json!("Profiles")]
    );
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    loaded.recompute();
    assert_eq!(listed(&loaded), listed(&doc));

    // One out of it again (the folder stays while Panel:1 names it), then
    // the other: the folder is gone.
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_folder", "occurrence": "Rail:1"}),
    );
    assert_eq!(doc.occurrence_folders(ComponentUid::ROOT), vec!["Profiles"]);
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_folder", "occurrence": "Panel:1", "folder": null}),
    );
    assert!(doc.occurrence_folders(ComponentUid::ROOT).is_empty());
    assert!(
        !doc.to_json().contains("\"folder\""),
        "no folder is left in the file"
    );
    assert_eq!(untouched(&doc), before);

    // The undo steps, one per command.
    for label in [
        "Take Panel:1 out of its Folder",
        "Take Rail:1 out of its Folder",
        "Rename Frame to Profiles",
        "Move Panel:1 to Frame",
        "Move Rail:1 to Frame",
    ] {
        assert_eq!(doc.undo().as_deref(), Some(label));
    }
    assert_eq!(
        listed(&doc),
        pairs(&[("Rail:1", ""), ("Post:1", ""), ("Panel:1", "")])
    );
    // The commands recomputed nothing; each undo recomputes the state it
    // restores, as every undo does (`Document::step`).
    assert_eq!(doc.recompute_count(), before.2 + 5);
}

#[test]
fn folder_commands_refuse_empty_and_taken_names() {
    let mut doc = three_components();
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_folder", "occurrence": "Rail:1", "folder": "Frame"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_folder", "occurrence": "Post:1", "folder": "Posts"}),
    );
    for (refused, why) in [
        (
            json!({"cmd": "set_occurrence_folder", "occurrence": "Rail:1", "folder": "  "}),
            "empty",
        ),
        (
            json!({"cmd": "set_occurrence_folder", "occurrence": "O9", "folder": "Frame"}),
            "no occurrence",
        ),
        (
            json!({"cmd": "rename_occurrence_folder", "component": "C0", "folder": "Frame", "name": "Posts"}),
            "already has a folder named Posts",
        ),
        (
            json!({"cmd": "rename_occurrence_folder", "component": "C0", "folder": "Glass", "name": "Panes"}),
            "no folder Glass",
        ),
        (
            json!({"cmd": "rename_occurrence_folder", "component": "C0", "folder": "Frame", "name": ""}),
            "empty",
        ),
    ] {
        let error = rejected(&mut doc, refused.clone());
        assert!(error.contains(why), "{refused}: {error}");
    }
    // The same name is no undo step.
    let depth = query(&doc, json!({"query": "document"}))["undo"].clone();
    command(
        &mut doc,
        json!({"cmd": "rename_occurrence_folder", "component": "C0", "folder": "Frame", "name": "Frame"}),
    );
    assert_eq!(query(&doc, json!({"query": "document"}))["undo"], depth);
}

#[test]
fn a_reparented_occurrence_leaves_its_folder() {
    let mut doc = three_components();
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_folder", "occurrence": "Post:1", "folder": "Frame"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_folder", "occurrence": "Panel:1", "folder": "Frame"}),
    );
    command(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": "O2", "parent": "O1"}),
    );
    // Post:1 is in Rail now, in no folder; Panel:1 keeps the root's.
    let tree = query(&doc, json!({"query": "components"}))["occurrences"].clone();
    let rail = &tree[0];
    assert_eq!(rail["name"], "Rail:1");
    assert_eq!(rail["children"][0]["name"], "Post:1");
    assert!(rail["children"][0].get("folder").is_none());
    assert_eq!(listed(&doc), pairs(&[("Rail:1", ""), ("Panel:1", "Frame")]));
    assert!(
        doc.occurrence_folders(ComponentUid(1)).is_empty(),
        "Rail has no folder"
    );
    // Undone, it is back in its folder.
    doc.undo();
    assert_eq!(
        listed(&doc),
        pairs(&[("Rail:1", ""), ("Post:1", "Frame"), ("Panel:1", "Frame")])
    );
}

#[test]
fn a_body_folder_is_renamed_in_one_undo_step() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    let pane = block(&mut doc, 0.0);
    let sash = block(&mut doc, 20.0);
    let frame = block(&mut doc, 40.0);
    for (body, folder) in [(pane, "Glass"), (sash, "Glass"), (frame, "Frame")] {
        command(
            &mut doc,
            json!({"cmd": "set_body_folder", "uid": body, "folder": folder}),
        );
    }
    let before = untouched(&doc);
    assert_eq!(doc.body_folders(ComponentUid::ROOT).len(), 2);

    command(
        &mut doc,
        json!({"cmd": "rename_body_folder", "component": "C0", "folder": "Glass", "name": "Panes"}),
    );
    assert_eq!(doc.body_attributes(pane).folder.as_deref(), Some("Panes"));
    assert_eq!(doc.body_attributes(sash).folder.as_deref(), Some("Panes"));
    assert_eq!(doc.body_attributes(frame).folder.as_deref(), Some("Frame"));
    assert_eq!(untouched(&doc), before);

    // Refused: a name the other folder has, a folder there is not.
    let error = rejected(
        &mut doc,
        json!({"cmd": "rename_body_folder", "component": "C0", "folder": "Panes", "name": "Frame"}),
    );
    assert!(
        error.contains("already has a folder named Frame"),
        "{error}"
    );
    let error = rejected(
        &mut doc,
        json!({"cmd": "rename_body_folder", "component": "C0", "folder": "Glass", "name": "Panes"}),
    );
    assert!(error.contains("no folder Glass"), "{error}");

    assert_eq!(doc.undo().as_deref(), Some("Rename Glass to Panes"));
    assert_eq!(doc.body_attributes(pane).folder.as_deref(), Some("Glass"));
    assert_eq!(doc.body_attributes(sash).folder.as_deref(), Some("Glass"));
}
