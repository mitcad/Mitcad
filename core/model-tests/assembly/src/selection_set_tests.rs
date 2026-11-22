// SPDX-License-Identifier: MIT
//! Selection sets (mitcad#2192) with the mock kernel: a set added, its
//! members added, removed and replaced, renamed (a taken name refused) and
//! deleted, each an undo step that leaves the timeline and the bill of
//! materials as they were and recomputes nothing; a member's item renamed
//! (its new name listed), suppressed or rolled back (inactive) and deleted
//! (gone from every set); the design file read back.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
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

/// A 10 x 10 x 5 block at `x` in the active component: its sketch, its
/// Extrude and its body.
fn block(doc: &mut Document<MockKernel>, x: f64) -> (FeatureUid, FeatureUid, BodyUid) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [x, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (sketch, uid, BodyUid::new(uid, 0))
}

/// Rail:1 (O1), Post:1 (O2) and Panel:1 (O3) in the root, each with a
/// block, and a block of the root's own.
struct Design {
    doc: Document<MockKernel>,
    /// The root's block.
    sketch: FeatureUid,
    extrude: FeatureUid,
    body: BodyUid,
    /// The bodies of Rail, Post and Panel.
    parts: Vec<BodyUid>,
}

fn design() -> Design {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true); // the bill of materials measures
    let mut parts = Vec::new();
    for (i, name) in ["Rail", "Post", "Panel"].iter().enumerate() {
        doc.create_component(Some(name), Transform::IDENTITY, true)
            .unwrap();
        parts.push(block(&mut doc, 20.0 * i as f64).2);
        doc.activate_component(ComponentUid::ROOT).unwrap();
    }
    let (sketch, extrude, body) = block(&mut doc, -40.0);
    Design {
        doc,
        sketch,
        extrude,
        body,
        parts,
    }
}

/// What a set command must leave as it was: the timeline, the bill of
/// materials and the number of recomputes.
fn untouched(doc: &Document<MockKernel>) -> (Value, Value, u64) {
    (
        query(doc, json!({"query": "timeline"})),
        query(doc, json!({"query": "parts_list"})),
        doc.recompute_count(),
    )
}

fn sets(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "selection_sets"}))
        .as_array()
        .unwrap()
        .clone()
}

/// A set's members as `kind name` (`body Body4`, `component Rail:1`), and
/// `(inactive)` after an inactive one.
fn members(doc: &Document<MockKernel>, set: &str) -> Vec<String> {
    let all = sets(doc);
    let set = all
        .iter()
        .find(|s| s["name"] == set)
        .unwrap_or_else(|| panic!("no set {set}"));
    set["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let mut text = format!(
                "{} {}",
                m["kind"].as_str().unwrap(),
                m["name"].as_str().unwrap()
            );
            if m["active"] != true {
                text.push_str(" (inactive)");
            }
            text
        })
        .collect()
}

fn undo_depth(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

#[test]
fn a_set_is_added_changed_renamed_deleted_and_undone() {
    let Design {
        mut doc,
        sketch,
        extrude,
        body,
        parts,
    } = design();
    let before = untouched(&doc);
    assert!(sets(&doc).is_empty());
    let body_name = doc.body_name(body);
    let sketch_name = doc.feature(sketch).unwrap().name.clone();
    let extrude_name = doc.feature(extrude).unwrap().name.clone();
    let post_name = doc.body_name(parts[1]);

    // A body, a sketch and a feature; the next free name.
    let reply = command(
        &mut doc,
        json!({"cmd": "add_selection_set", "members": [
            {"body": body}, {"feature": sketch}, {"feature": extrude_name}
        ]}),
    );
    // (The answer carries the api's own fields too: recomputed, error.)
    assert_eq!(reply["name"], "SelectionSet1");
    assert_eq!(reply["members"], 3);
    let reply = command(&mut doc, json!({"cmd": "add_selection_set"}));
    assert_eq!(reply["name"], "SelectionSet2");
    assert_eq!(
        members(&doc, "SelectionSet1"),
        vec![
            format!("body {body_name}"),
            format!("feature {sketch_name}"),
            format!("feature {extrude_name}")
        ]
    );
    assert_eq!(untouched(&doc), before);

    // Added: an occurrence and a body it places; one already in it is not
    // added again.
    let reply = command(
        &mut doc,
        json!({"cmd": "set_selection_set_members", "name": "SelectionSet1", "add": [
            {"occurrence": "Rail:1"}, {"body": parts[1], "occurrence": "O2"}, {"body": body}
        ]}),
    );
    assert_eq!(reply["members"], 5);
    let listed = sets(&doc);
    let post = &listed[0]["members"][4];
    assert_eq!(post["kind"], "body");
    assert_eq!(post["name"], post_name.as_str());
    assert_eq!(post["occurrence"], "O2");
    assert_eq!(post["path_name"], "Post:1");
    assert_eq!(post["component"], "C2");
    assert_eq!(post["exists"], true);
    let rail = &listed[0]["members"][3];
    assert_eq!(rail["kind"], "component");
    assert_eq!(rail["name"], "Rail:1");
    assert_eq!(rail["occurrence"], "O1");
    assert_eq!(listed[0]["members"][1]["type"], "sketch");
    assert_eq!(listed[0]["members"][2]["type"], "extrude");

    // Removed, then replaced.
    let reply = command(
        &mut doc,
        json!({"cmd": "set_selection_set_members", "name": "SelectionSet1", "remove": [{"feature": sketch}]}),
    );
    assert_eq!(reply["members"], 4);
    let reply = command(
        &mut doc,
        json!({"cmd": "set_selection_set_members", "name": "SelectionSet1", "members": [
            {"feature": extrude}, {"body": body}
        ]}),
    );
    assert_eq!(reply["members"], 2);
    // The same members again is no undo step.
    let depth = undo_depth(&doc);
    command(
        &mut doc,
        json!({"cmd": "set_selection_set_members", "name": "SelectionSet1", "members": [
            {"feature": extrude}, {"body": body}
        ]}),
    );
    assert_eq!(undo_depth(&doc), depth);
    assert_eq!(
        members(&doc, "SelectionSet1"),
        vec![
            format!("feature {extrude_name}"),
            format!("body {body_name}")
        ]
    );

    // Renamed; a taken name and an empty one are refused, the same name
    // is no undo step.
    command(
        &mut doc,
        json!({"cmd": "rename_selection_set", "name": "SelectionSet1", "new_name": " Favourites "}),
    );
    let error = rejected(
        &mut doc,
        json!({"cmd": "rename_selection_set", "name": "SelectionSet2", "new_name": "Favourites"}),
    );
    assert!(
        error.contains("a selection set Favourites exists"),
        "{error}"
    );
    let error = rejected(
        &mut doc,
        json!({"cmd": "rename_selection_set", "name": "SelectionSet2", "new_name": "  "}),
    );
    assert!(error.contains("needs a name"), "{error}");
    let depth = undo_depth(&doc);
    command(
        &mut doc,
        json!({"cmd": "rename_selection_set", "name": "Favourites", "new_name": "Favourites"}),
    );
    assert_eq!(undo_depth(&doc), depth);

    // Deleted: the set only, never its items.
    command(
        &mut doc,
        json!({"cmd": "delete_selection_set", "name": "SelectionSet2"}),
    );
    assert_eq!(sets(&doc).len(), 1);
    assert!(doc.feature(extrude).is_some());
    assert_eq!(untouched(&doc), before);

    // One undo step each, newest first.
    for label in [
        "Delete Selection Set SelectionSet2",
        "Rename Selection Set SelectionSet1",
        "Change Selection Set SelectionSet1",
        "Change Selection Set SelectionSet1",
        "Change Selection Set SelectionSet1",
        "Add Selection Set SelectionSet2",
        "Add Selection Set SelectionSet1",
    ] {
        assert_eq!(doc.undo().as_deref(), Some(label));
    }
    assert!(sets(&doc).is_empty());
    let (timeline, parts_list, _) = untouched(&doc);
    assert_eq!((timeline, parts_list), (before.0, before.1));
}

#[test]
fn members_follow_their_items_renamed_suppressed_and_rolled_back() {
    let Design {
        mut doc,
        sketch,
        extrude,
        body,
        ..
    } = design();
    command(
        &mut doc,
        json!({"cmd": "add_selection_set", "name": "Favourites", "members": [
            {"body": body}, {"feature": sketch}, {"feature": extrude}
        ]}),
    );

    // Renamed items show their new names: members are kept by reference.
    command(
        &mut doc,
        json!({"cmd": "rename_body", "uid": body, "name": "Lid"}),
    );
    command(
        &mut doc,
        json!({"cmd": "rename_feature", "uid": extrude, "name": "Lid extrude"}),
    );
    let sketch_name = doc.feature(sketch).unwrap().name.clone();
    assert_eq!(
        members(&doc, "Favourites"),
        vec![
            "body Lid".to_owned(),
            format!("feature {sketch_name}"),
            "feature Lid extrude".to_owned()
        ]
    );

    // Suppressed: the feature and the body it made are inactive, still
    // members.
    command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": extrude, "suppressed": true}),
    );
    assert_eq!(
        members(&doc, "Favourites"),
        vec![
            "body Lid (inactive)".to_owned(),
            format!("feature {sketch_name}"),
            "feature Lid extrude (inactive)".to_owned()
        ]
    );
    assert!(sets(&doc)[0]["members"][0]["exists"] == true);
    doc.undo();

    // Rolled back past both: inactive too.
    let at = doc.state().position(sketch).unwrap();
    command(&mut doc, json!({"cmd": "set_marker", "position": at}));
    assert_eq!(
        members(&doc, "Favourites"),
        vec![
            "body Lid (inactive)".to_owned(),
            format!("feature {sketch_name} (inactive)"),
            "feature Lid extrude (inactive)".to_owned()
        ]
    );
}

#[test]
fn a_deleted_item_leaves_every_set() {
    let Design {
        mut doc,
        sketch,
        extrude,
        body,
        parts,
    } = design();
    command(
        &mut doc,
        json!({"cmd": "add_selection_set", "name": "A", "members": [
            {"body": body}, {"feature": sketch}, {"feature": extrude},
            {"occurrence": "Panel:1"}, {"body": parts[2], "occurrence": "O3"}, {"occurrence": "Rail:1"}
        ]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_selection_set", "name": "B", "members": [{"feature": extrude}, {"body": body}]}),
    );

    // The Extrude deleted: it and its body leave both sets in its step.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": extrude}));
    let sketch_name = doc.feature(sketch).unwrap().name.clone();
    assert_eq!(
        members(&doc, "A"),
        vec![
            format!("feature {sketch_name}"),
            "component Panel:1".to_owned(),
            format!("body {}", doc.body_name(parts[2])),
            "component Rail:1".to_owned()
        ]
    );
    assert!(members(&doc, "B").is_empty());

    // Panel:1 deleted: it and the body it places leave.
    command(
        &mut doc,
        json!({"cmd": "delete_occurrence", "occurrence": "Panel:1"}),
    );
    assert_eq!(
        members(&doc, "A"),
        vec![
            format!("feature {sketch_name}"),
            "component Rail:1".to_owned()
        ]
    );
    // Saved and read back as it is.
    let json = doc.to_json();
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(sets(&loaded), sets(&doc));

    // Undone, they are back.
    doc.undo();
    doc.undo();
    assert_eq!(members(&doc, "A").len(), 6);
    assert_eq!(members(&doc, "B").len(), 2);
}

#[test]
fn selection_sets_are_saved_and_read_back() {
    let Design {
        mut doc,
        extrude,
        body,
        parts,
        ..
    } = design();
    assert!(!doc.to_json().contains("selection_sets"));
    command(
        &mut doc,
        json!({"cmd": "add_selection_set", "name": "Favourites", "members": [
            {"body": body}, {"feature": extrude}, {"occurrence": "O1"},
            {"body": parts[1], "occurrence": "Post:1"}
        ]}),
    );
    let json = doc.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        file["selection_sets"],
        json!([{"name": "Favourites", "members": [
            {"body": body}, {"feature": extrude}, {"occurrence": "O1"},
            {"body": parts[1], "occurrence": "O2"}
        ]}])
    );
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    loaded.recompute();
    assert_eq!(sets(&loaded), sets(&doc));

    // A member whose item a file does not have is left out when read.
    let mut file = file;
    file["selection_sets"][0]["members"]
        .as_array_mut()
        .unwrap()
        .push(json!({"feature": "F99"}));
    let loaded = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    assert_eq!(loaded.selection_sets()[0].members.len(), 4);
    // A name listed twice is refused.
    let mut twice = file.clone();
    let first = twice["selection_sets"][0].clone();
    twice["selection_sets"].as_array_mut().unwrap().push(first);
    assert!(Document::from_json(&twice.to_string(), MockKernel::default()).is_err());
}

#[test]
fn members_not_in_the_design_are_refused() {
    let Design {
        mut doc,
        extrude,
        parts,
        ..
    } = design();
    command(
        &mut doc,
        json!({"cmd": "add_selection_set", "name": "Favourites"}),
    );
    for (refused, why) in [
        (
            json!({"cmd": "add_selection_set", "name": "Favourites"}),
            "a selection set Favourites exists",
        ),
        (
            json!({"cmd": "add_selection_set", "members": [{"body": "F99.b0"}]}),
            "no body F99.b0",
        ),
        (
            json!({"cmd": "add_selection_set", "members": [{"feature": "F99"}]}),
            "F99",
        ),
        (
            json!({"cmd": "add_selection_set", "members": [{"occurrence": "O9"}]}),
            "no occurrence path O9",
        ),
        (
            json!({"cmd": "add_selection_set", "members": [{}]}),
            "needs a body, a feature or an occurrence",
        ),
        (
            json!({"cmd": "add_selection_set", "members": [{"body": parts[0], "feature": extrude}]}),
            "not both",
        ),
        // The root's Extrude is not in Rail, nor Rail's body in the root.
        (
            json!({"cmd": "set_selection_set_members", "name": "Favourites", "add": [{"feature": extrude, "occurrence": "O1"}]}),
            "is not in Rail:1",
        ),
        (
            json!({"cmd": "set_selection_set_members", "name": "Favourites", "add": [{"body": parts[0]}]}),
            "is not in",
        ),
        (
            json!({"cmd": "set_selection_set_members", "name": "Nothing", "add": []}),
            "there is no selection set Nothing",
        ),
        (
            json!({"cmd": "delete_selection_set", "name": "Nothing"}),
            "there is no selection set Nothing",
        ),
    ] {
        let error = rejected(&mut doc, refused.clone());
        assert!(error.contains(why), "{refused}: {error}");
    }
    assert_eq!(sets(&doc).len(), 1);
}
