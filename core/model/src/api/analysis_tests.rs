// SPDX-License-Identifier: MIT
//! Analyses kept in the document (mitcad#41): section analyses added,
//! edited, shown and hidden one at a time, renamed and deleted as undo
//! steps, their planes following the model, and saved in the project file.

use serde_json::{Value, json};

use crate::Document;
use crate::diff::diff_states;
use crate::testing::MockKernel;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn analyses(doc: &Document<MockKernel>) -> Vec<Value> {
    let answer: Value =
        serde_json::from_str(&doc.query(r#"{"query": "analyses"}"#).unwrap()).unwrap();
    answer.as_array().unwrap().clone()
}

/// The analyses as `name` or `name*` when shown.
fn shown(doc: &Document<MockKernel>) -> Vec<String> {
    analyses(doc)
        .iter()
        .map(|a| {
            let name = a["name"].as_str().unwrap();
            if a["visible"].as_bool().unwrap() {
                format!("{name}*")
            } else {
                name.to_owned()
            }
        })
        .collect()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    let answer: Value =
        serde_json::from_str(&doc.query(r#"{"query": "document"}"#).unwrap()).unwrap();
    answer["undo"].clone()
}

fn section(plane: Value, offset: f64, flip: bool) -> Value {
    json!({"type": "section", "plane": plane, "offset": offset, "flip": flip})
}

/// A 20 x 20 x 20 block (F2.b0) on a rectangle on XY.
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc
}

#[test]
fn section_analyses_are_added_edited_shown_one_at_a_time_and_undone() {
    let mut doc = block();
    assert!(analyses(&doc).is_empty());
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("yz"), 10.0, false)}),
    );
    assert_eq!(added["name"], "Section1");
    assert_eq!(added["recomputed"], 0, "an analysis changes no geometry");
    assert_eq!(undo_label(&doc), "Add Section1");
    let first = &analyses(&doc)[0];
    assert_eq!(first["type"], "section");
    assert_eq!(first["plane"], "yz");
    assert_eq!(first["offset"], 10.0);
    assert_eq!(
        first["section"],
        json!({"origin": [10.0, 0.0, 0.0], "normal": [1.0, 0.0, 0.0]})
    );

    // A second one is shown instead of the first.
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("xy"), 0.0, true)}),
    );
    assert_eq!(added["name"], "Section2");
    assert_eq!(shown(&doc), ["Section1", "Section2*"]);
    // Flipped: the normal reversed, without -0.
    assert_eq!(
        analyses(&doc)[1]["section"],
        json!({"origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, -1.0]})
    );
    command(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Section1", "visible": true}),
    );
    assert_eq!(shown(&doc), ["Section1*", "Section2"]);
    assert_eq!(undo_label(&doc), "Show Section1");
    // Nothing to change: no undo step.
    command(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Section1", "visible": true}),
    );
    assert_eq!(undo_label(&doc), "Show Section1");
    command(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Section1", "visible": false}),
    );
    assert_eq!(shown(&doc), ["Section1", "Section2"]);
    assert_eq!(undo_label(&doc), "Hide Section1");

    // Editing keeps it hidden unless asked to show it.
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Section1", "def": section(json!("yz"), 5.0, true)}),
    );
    assert_eq!(shown(&doc), ["Section1", "Section2"]);
    assert_eq!(undo_label(&doc), "Edit Section1");
    assert_eq!(
        analyses(&doc)[0]["section"],
        json!({"origin": [5.0, 0.0, 0.0], "normal": [-1.0, 0.0, 0.0]})
    );
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Section2", "def": section(json!("xy"), 0.0, true),
               "visible": true}),
    );
    assert_eq!(shown(&doc), ["Section1", "Section2*"]);
    assert_eq!(undo_label(&doc), "Edit Section2");

    command(
        &mut doc,
        json!({"cmd": "rename_analysis", "name": "Section1", "new_name": "Middle"}),
    );
    assert_eq!(shown(&doc), ["Middle", "Section2*"]);
    // The next default name is still free.
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("xz"), 0.0, false), "name": "  "}),
    );
    assert_eq!(shown(&doc), ["Middle", "Section2", "Section1*"]);
    command(
        &mut doc,
        json!({"cmd": "delete_analysis", "name": "Section2"}),
    );
    assert_eq!(shown(&doc), ["Middle", "Section1*"]);
    assert_eq!(undo_label(&doc), "Delete Section2");

    // Undo and redo take them back as other document commands.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(shown(&doc), ["Middle", "Section2", "Section1*"]);
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(shown(&doc), ["Section1", "Section2*"]);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(shown(&doc), ["Middle", "Section2*"]);
}

#[test]
fn analyses_refuse_what_they_cannot_keep() {
    let mut doc = block();
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("x"), 0.0, false)}),
    );
    assert!(error.contains("the section's plane"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!({"body": "F2.b0", "face": "F2:end(r{c9})"}), 0.0, false)}),
    );
    assert!(error.contains("F2:end(r{c9})"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xy", "depth": 3}}),
    );
    assert!(error.contains("depth"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("xy"), 0.0, false), "name": "Cut"}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("xy"), 0.0, false), "name": "Cut"}),
    );
    assert_eq!(error, "an analysis 'Cut' exists");
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("yz"), 0.0, false)}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "rename_analysis", "name": "Section1", "new_name": "Cut"}),
    );
    assert_eq!(error, "an analysis 'Cut' exists");
    let error = refused(
        &mut doc,
        json!({"cmd": "rename_analysis", "name": "Section1", "new_name": " "}),
    );
    assert_eq!(error, "the analysis needs a name");
    let error = refused(&mut doc, json!({"cmd": "delete_analysis", "name": "Nope"}));
    assert_eq!(error, "there is no analysis 'Nope'");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Nope", "visible": true}),
    );
    assert_eq!(error, "there is no analysis 'Nope'");
}

#[test]
fn a_section_on_a_face_follows_the_model() {
    let mut doc = block();
    let top = json!({"body": "F2.b0", "face": format!("F2:end({R})")});
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(top.clone(), -5.0, false)}),
    );
    assert_eq!(
        analyses(&doc)[0]["section"],
        json!({"origin": [0.0, 0.0, 15.0], "normal": [0.0, 0.0, 1.0]})
    );
    // The block made taller: the face is found again by its name.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": R}],
               "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}}),
    );
    assert_eq!(
        analyses(&doc)[0]["section"],
        json!({"origin": [0.0, 0.0, 25.0], "normal": [0.0, 0.0, 1.0]})
    );
    assert_eq!(analyses(&doc)[0]["plane"], top);
    // Rolled back before the block, the face is not there: the analysis
    // says why, and stays.
    command(&mut doc, json!({"cmd": "set_marker", "position": 1}));
    let rolled = &analyses(&doc)[0];
    assert!(rolled.get("section").is_none(), "{rolled}");
    assert!(
        rolled["error"].as_str().unwrap().contains("F2.b0"),
        "{rolled}"
    );
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    assert!(analyses(&doc)[0].get("section").is_some());
}

#[test]
fn analyses_are_saved_and_read_back() {
    let mut doc = block();
    let plain = doc.to_json();
    assert!(!plain.contains("\"analyses\""), "none: left out");
    let before = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!("yz"), 10.0, true)}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": section(json!({"body": "F2.b0", "face": format!("F2:end({R})")}), 0.0, false)}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["analyses"],
        json!([
            {"name": "Section1", "visible": false, "type": "section", "plane": "yz", "offset": 10.0,
             "flip": true},
            {"name": "Section2", "type": "section",
             "plane": {"body": "F2.b0", "face": format!("F2:end({R})")}}
        ])
    );
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
    assert_eq!(shown(&loaded), ["Section1", "Section2*"]);

    // Older files without analyses load as before.
    let old = Document::from_json(&plain, MockKernel::default()).unwrap();
    assert!(analyses(&old).is_empty());

    // The comparison of versions lists them.
    let diff = diff_states(&before, doc.state());
    let texts: Vec<&str> = diff.analyses.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["Section1: added", "Section2: added"]);
    assert!(diff.summary.contains("+2 analyses"), "{}", diff.summary);

    // One shown at a time, also from a file: the first shown one.
    let both = saved.replace("\"visible\": false,\n", "");
    let loaded = Document::from_json(&both, MockKernel::default()).unwrap();
    assert_eq!(shown(&loaded), ["Section1*", "Section2"]);

    // Broken entries are refused with where they are.
    let twice = saved.replace("\"Section2\"", "\"Section1\"");
    let Err(error) = Document::from_json(&twice, MockKernel::default()) else {
        panic!("a name listed twice was read");
    };
    assert!(error.to_string().contains("analyses[1]"), "{error}");
    let unknown = saved.replace("\"offset\": 10.0", "\"depth\": 10.0");
    let Err(error) = Document::from_json(&unknown, MockKernel::default()) else {
        panic!("an unknown field was read");
    };
    assert!(error.to_string().contains("analyses[0]"), "{error}");
}
