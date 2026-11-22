// SPDX-License-Identifier: MIT
//! Groups of user parameters (mitcad#2180) through the api: nested paths,
//! groups made before they are filled, parameters added into a group and
//! moved between groups, a group renamed and deleted with its parameters
//! in it, each an undo step that recomputes nothing, the `parameters` and
//! `parameter_groups` queries, and the design file (a file written before
//! groups reads with every parameter ungrouped).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::test_support::block;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn doc() -> Document<MockKernel> {
    Document::new(MockKernel::default())
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(error) => error.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// Each parameter's name and group (`""` for none), in the query's order.
fn grouped(doc: &Document<MockKernel>) -> Vec<(String, String)> {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap().to_owned(),
                p["group"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

/// The groups' paths, in the `parameter_groups` query's order.
fn paths(doc: &Document<MockKernel>) -> Vec<String> {
    query(doc, json!({"query": "parameter_groups"}))["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["path"].as_str().unwrap().to_owned())
        .collect()
}

fn pairs(rows: &[(&str, &str)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

/// width, depth and height of 10, 20 and 30 mm, ungrouped.
fn three() -> Document<MockKernel> {
    let mut doc = doc();
    for (name, value) in [("width", 10.0), ("depth", 20.0), ("height", 30.0)] {
        command(
            &mut doc,
            json!({"cmd": "add_parameter", "name": name, "value": value}),
        );
    }
    doc
}

#[test]
fn groups_nest_and_are_made_before_they_are_filled() {
    let mut doc = three();
    assert_eq!(paths(&doc), Vec::<String>::new());
    // A nested path makes its parents; spaces around the names go.
    let made = command(
        &mut doc,
        json!({"cmd": "add_parameter_group", "group": " Cabinet / Doors "}),
    );
    assert_eq!(made["group"], "Cabinet/Doors");
    assert_eq!(doc.undo_label(), Some("Add Parameter Group Cabinet/Doors"));
    command(
        &mut doc,
        json!({"cmd": "add_parameter_group", "group": "Material"}),
    );
    assert_eq!(paths(&doc), ["Cabinet", "Cabinet/Doors", "Material"]);
    let listed = query(&doc, json!({"query": "parameter_groups"}));
    assert_eq!(
        listed["groups"][1],
        json!({"path": "Cabinet/Doors", "name": "Doors", "parent": "Cabinet", "parameters": []})
    );
    // An existing group, an empty name and an empty name in a path.
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "add_parameter_group", "group": "Cabinet"})
        )
        .contains("already exists")
    );
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "add_parameter_group", "group": " "})
        )
        .contains("the group name is empty")
    );
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "add_parameter_group", "group": "Cabinet//Doors"})
        )
        .contains("empty group name")
    );

    // A parameter added into a group, in the same undo step; its group is
    // in the query.
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "door", "value": 4, "group": "Cabinet/Doors"}),
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(doc.undo_label(), Some("Add Parameter door"));
    // And one into a group that did not exist: made with it.
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "top", "value": 2, "group": "Table/Top"}),
    );
    assert_eq!(
        paths(&doc),
        ["Cabinet", "Cabinet/Doors", "Material", "Table", "Table/Top"]
    );
    assert_eq!(
        grouped(&doc),
        pairs(&[
            ("width", ""),
            ("depth", ""),
            ("height", ""),
            ("door", "Cabinet/Doors"),
            ("top", "Table/Top"),
        ])
    );
    // Undo takes the parameter and the group it made away together.
    doc.undo();
    assert_eq!(paths(&doc), ["Cabinet", "Cabinet/Doors", "Material"]);
    assert!(doc.parameters().find("top").is_none());
}

#[test]
fn parameters_move_between_groups_in_one_undo_step_each() {
    let mut doc = three();
    let revision = query(&doc, json!({"query": "document"}))["revision"].clone();
    command(
        &mut doc,
        json!({"cmd": "set_parameter_group", "names": ["width", "depth"], "group": "Cabinet"}),
    );
    assert_eq!(doc.undo_label(), Some("Move 2 Parameters to Group Cabinet"));
    assert_eq!(
        grouped(&doc),
        pairs(&[("width", "Cabinet"), ("depth", "Cabinet"), ("height", "")])
    );
    assert_ne!(
        query(&doc, json!({"query": "document"}))["revision"],
        revision
    );
    // set_parameter's group moves one; the same group again is no step.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "group": "Cabinet/Doors"}),
    );
    assert_eq!(doc.undo_label(), Some("Move height to Group Cabinet/Doors"));
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "group": "Cabinet/Doors"}),
    );
    assert_eq!(doc.undo_depth(), depth);
    // Out of its group.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "width", "group": ""}),
    );
    assert_eq!(doc.undo_label(), Some("Take width out of its Group"));
    assert_eq!(
        grouped(&doc),
        pairs(&[
            ("width", ""),
            ("depth", "Cabinet"),
            ("height", "Cabinet/Doors")
        ])
    );
    // Undo goes back step by step.
    doc.undo();
    doc.undo();
    assert_eq!(
        grouped(&doc),
        pairs(&[("width", "Cabinet"), ("depth", "Cabinet"), ("height", "")])
    );
    assert_eq!(paths(&doc), ["Cabinet"]);
    doc.undo();
    assert_eq!(
        grouped(&doc),
        pairs(&[("width", ""), ("depth", ""), ("height", "")])
    );
    assert_eq!(paths(&doc), Vec::<String>::new());
    doc.redo();
    assert_eq!(
        grouped(&doc),
        pairs(&[("width", "Cabinet"), ("depth", "Cabinet"), ("height", "")])
    );
    // An unknown name refuses the whole move.
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "set_parameter_group", "names": ["height", "nothing"], "group": "Cabinet"})
        )
        .contains("nothing")
    );
    assert_eq!(grouped(&doc)[2], ("height".to_owned(), String::new()));
}

#[test]
fn a_renamed_group_takes_its_subgroups_and_parameters_along() {
    let mut doc = three();
    command(
        &mut doc,
        json!({"cmd": "set_parameter_group", "names": ["width"], "group": "Cabinet"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter_group", "names": ["depth"], "group": "Cabinet/Doors"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter_group", "group": "Material"}),
    );
    let renamed = command(
        &mut doc,
        json!({"cmd": "rename_parameter_group", "group": "Cabinet", "new_group": "Table"}),
    );
    assert_eq!(renamed["group"], "Table");
    assert_eq!(
        doc.undo_label(),
        Some("Rename Parameter Group Cabinet to Table")
    );
    assert_eq!(paths(&doc), ["Table", "Table/Doors", "Material"]);
    assert_eq!(
        grouped(&doc),
        pairs(&[("width", "Table"), ("depth", "Table/Doors"), ("height", "")])
    );
    // A new path may put it under another group.
    command(
        &mut doc,
        json!({"cmd": "rename_parameter_group", "group": "Table/Doors", "new_group": "Material/Fronts"}),
    );
    assert_eq!(paths(&doc), ["Table", "Material", "Material/Fronts"]);
    assert_eq!(grouped(&doc)[1].1, "Material/Fronts");
    // Refused: a path another group has, within itself, a group that does
    // not exist. The same path is no step.
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "rename_parameter_group", "group": "Table", "new_group": "Material"})
        )
        .contains("already exists")
    );
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "rename_parameter_group", "group": "Material", "new_group": "Material/Inner"})
        )
        .contains("into itself")
    );
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "rename_parameter_group", "group": "Nothing", "new_group": "Else"})
        )
        .contains("no parameter group Nothing")
    );
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "rename_parameter_group", "group": "Table", "new_group": " Table "}),
    );
    assert_eq!(doc.undo_depth(), depth);
    // Undo: the old paths and groups back.
    doc.undo();
    doc.undo();
    assert_eq!(paths(&doc), ["Cabinet", "Cabinet/Doors", "Material"]);
    assert_eq!(
        grouped(&doc),
        pairs(&[
            ("width", "Cabinet"),
            ("depth", "Cabinet/Doors"),
            ("height", "")
        ])
    );
}

#[test]
fn a_deleted_group_moves_its_parameters_up_a_level() {
    let mut doc = three();
    command(
        &mut doc,
        json!({"cmd": "set_parameter_group", "names": ["width", "depth"], "group": "Cabinet"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter_group", "names": ["height"], "group": "Cabinet/Doors"}),
    );
    // A subgroup's parameters go to its parent.
    let deleted = command(
        &mut doc,
        json!({"cmd": "delete_parameter_group", "group": "Cabinet/Doors"}),
    );
    assert_eq!(deleted["moved"], json!(["height"]));
    assert_eq!(
        doc.undo_label(),
        Some("Delete Parameter Group Cabinet/Doors")
    );
    assert_eq!(paths(&doc), ["Cabinet"]);
    assert_eq!(
        grouped(&doc)[2],
        ("height".to_owned(), "Cabinet".to_owned())
    );
    doc.undo();
    // A group at the top: its parameters are ungrouped, its subgroups move
    // up a level; no parameter is deleted.
    let deleted = command(
        &mut doc,
        json!({"cmd": "delete_parameter_group", "group": "Cabinet"}),
    );
    assert_eq!(deleted["moved"], json!(["width", "depth"]));
    assert_eq!(paths(&doc), ["Doors"]);
    assert_eq!(
        grouped(&doc),
        pairs(&[("width", ""), ("depth", ""), ("height", "Doors")])
    );
    doc.undo();
    assert_eq!(paths(&doc), ["Cabinet", "Cabinet/Doors"]);
    assert_eq!(
        grouped(&doc),
        pairs(&[
            ("width", "Cabinet"),
            ("depth", "Cabinet"),
            ("height", "Cabinet/Doors")
        ])
    );
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "delete_parameter_group", "group": "Nothing"})
        )
        .contains("no parameter group Nothing")
    );
}

#[test]
fn only_user_parameters_have_groups() {
    let mut doc = block().doc;
    // d1 is the rectangle's dimension: a model parameter.
    assert!(
        refused(
            &mut doc,
            json!({"cmd": "set_parameter", "name": "d1", "group": "Cabinet"})
        )
        .contains("only user parameters go in groups")
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "gap", "value": 2, "group": "Cabinet"}),
    );
    // A deleted parameter leaves its group; the group stays.
    command(&mut doc, json!({"cmd": "delete_parameter", "name": "gap"}));
    assert_eq!(paths(&doc), ["Cabinet"]);
    assert!(
        query(&doc, json!({"query": "parameter_groups"}))["groups"][0]["parameters"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // A renamed parameter keeps its group.
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "gap", "value": 2, "group": "Cabinet"}),
    );
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "gap", "new_name": "clearance"}),
    );
    assert_eq!(doc.parameter_group("clearance"), Some("Cabinet"));
}

#[test]
fn groups_are_saved_and_read_back() {
    let mut doc = three();
    command(
        &mut doc,
        json!({"cmd": "set_parameter_group", "names": ["depth"], "group": "Cabinet/Doors"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter_group", "group": "Material"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "width", "group": "Cabinet"}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["parameter_groups"],
        json!(["Cabinet", "Cabinet/Doors", "Material"])
    );
    assert_eq!(file["parameters"][0]["group"], "Cabinet");
    assert_eq!(file["parameters"][1]["group"], "Cabinet/Doors");
    assert!(file["parameters"][2].get("group").is_none());
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    assert_eq!(grouped(&loaded), grouped(&doc));
    // The empty group Material too.
    assert_eq!(paths(&loaded), ["Cabinet", "Cabinet/Doors", "Material"]);
    assert_eq!(loaded.parameter_groups(), doc.parameter_groups());
}

#[test]
fn a_design_saved_before_groups_opens_with_every_parameter_ungrouped() {
    let doc = three();
    let saved = doc.to_json();
    // Nothing of groups is written for a design without them, as a file
    // written before them has none.
    assert!(!saved.contains("\"group\":"));
    assert!(!saved.contains("parameter_groups"));
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(
        grouped(&loaded),
        pairs(&[("width", ""), ("depth", ""), ("height", "")])
    );
    assert_eq!(paths(&loaded), Vec::<String>::new());
    // A file whose group path is broken is refused with where.
    let mut file: Value = serde_json::from_str(&saved).unwrap();
    file["parameter_groups"] = json!(["Cabinet//Doors"]);
    let error = Document::from_json(&file.to_string(), MockKernel::default())
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("parameter_groups"), "{error}");
}
