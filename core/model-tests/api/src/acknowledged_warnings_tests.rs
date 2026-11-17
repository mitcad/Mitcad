// SPDX-License-Identifier: MIT
//! Acknowledged warnings (mitcad#1935, GitHub mitcad/Mitcad#380): the
//! `acknowledge_warnings` command, the timeline's `acknowledged`, undo, the
//! design file, and the acknowledgement lapsing when the warning changes
//! or the feature fails.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::test_support::{Block, block, corner};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const NOTE: &str = "the corner of the fillet folds back on itself";
const OTHER: &str = "the fillet leaves a sliver";

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

fn item(doc: &Document<MockKernel>, name: &str) -> Value {
    let timeline: Value = serde_json::from_str(
        &doc.query(&json!({"query": "timeline"}).to_string())
            .unwrap(),
    )
    .unwrap();
    timeline["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no {name} in the timeline"))
}

fn acknowledged(doc: &Document<MockKernel>, name: &str) -> bool {
    item(doc, name)["acknowledged"] == json!(true)
}

/// The block (Extrude1 F2) with Fillet1 (F3), whose result the kernel
/// notes `NOTE` on: a warning.
fn warned() -> Block {
    let mut b = block();
    b.doc
        .kernel()
        .notes
        .borrow_mut()
        .insert("fillet", NOTE.to_owned());
    let fillet = json!({"type": "fillet", "body": "F2.b0",
                        "edges": [corner(b.extrude, 1, 0)], "radius": 2});
    command(&mut b.doc, json!({"cmd": "add_feature", "def": fillet}));
    assert_eq!(item(&b.doc, "Fillet1")["status"], "warning");
    b
}

/// Fillet1 edited to `radius`, which evaluates it again.
fn set_radius(b: &mut Block, radius: f64) {
    let fillet = json!({"type": "fillet", "body": "F2.b0",
                        "edges": [corner(b.extrude, 1, 0)], "radius": radius});
    command(
        &mut b.doc,
        json!({"cmd": "edit_feature", "uid": "F3", "def": fillet}),
    );
}

#[test]
fn an_acknowledged_warning_is_kept_with_undo_and_in_the_file() {
    let mut b = warned();
    assert!(!acknowledged(&b.doc, "Fillet1"));

    command(
        &mut b.doc,
        json!({"cmd": "acknowledge_warnings", "uids": ["F3"]}),
    );
    let fillet = item(&b.doc, "Fillet1");
    assert_eq!(fillet["acknowledged"], json!(true), "{fillet}");
    // Still a warning for agents: its status, level and notices.
    assert_eq!(fillet["status"], "warning");
    assert_eq!(fillet["level"], "warning");
    assert_eq!(fillet["notices"][0]["message"], NOTE);
    assert_eq!(b.doc.undo_label(), Some("Acknowledge Fillet1"));
    assert!(item(&b.doc, "Extrude1").get("acknowledged").is_none());

    // Again: nothing changes, no undo step.
    let depth = b.doc.undo_depth();
    command(
        &mut b.doc,
        json!({"cmd": "acknowledge_warnings", "uids": ["F3"], "acknowledged": true}),
    );
    assert_eq!(b.doc.undo_depth(), depth);

    // Saved with the feature, still acknowledged when opened.
    let saved = b.doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["features"][2]["acknowledged_warning"], NOTE);
    assert!(file["features"][1].get("acknowledged_warning").is_none());
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded
        .kernel()
        .notes
        .borrow_mut()
        .insert("fillet", NOTE.to_owned());
    loaded.recompute();
    assert!(acknowledged(&loaded, "Fillet1"));

    // Shown again: one undo step; undo acknowledges it again.
    command(
        &mut b.doc,
        json!({"cmd": "acknowledge_warnings", "uids": ["F3"], "acknowledged": false}),
    );
    assert!(!acknowledged(&b.doc, "Fillet1"));
    assert_eq!(b.doc.undo_label(), Some("Unacknowledge Fillet1"));
    assert!(!b.doc.to_json().contains("acknowledged_warning"));
    command(&mut b.doc, json!({"cmd": "undo"}));
    assert!(acknowledged(&b.doc, "Fillet1"));
    command(&mut b.doc, json!({"cmd": "undo"}));
    assert!(!acknowledged(&b.doc, "Fillet1"));

    // Deleting the feature forgets it.
    command(&mut b.doc, json!({"cmd": "redo"}));
    command(&mut b.doc, json!({"cmd": "delete_feature", "uid": "F3"}));
    assert!(!b.doc.to_json().contains("acknowledged_warning"));
}

#[test]
fn an_acknowledgement_lapses_when_the_warning_changes() {
    let mut b = warned();
    command(
        &mut b.doc,
        json!({"cmd": "acknowledge_warnings", "uids": ["F3"]}),
    );
    assert!(acknowledged(&b.doc, "Fillet1"));

    // Another warning text: shown again, and forgotten in that edit.
    b.doc
        .kernel()
        .notes
        .borrow_mut()
        .insert("fillet", OTHER.to_owned());
    set_radius(&mut b, 3.0);
    let fillet = item(&b.doc, "Fillet1");
    assert_eq!(fillet["status"], "warning");
    assert!(fillet.get("acknowledged").is_none(), "{fillet}");
    assert!(!b.doc.to_json().contains("acknowledged_warning"));

    // The first warning back: still shown, the acknowledgement lapsed.
    b.doc
        .kernel()
        .notes
        .borrow_mut()
        .insert("fillet", NOTE.to_owned());
    set_radius(&mut b, 2.0);
    assert_eq!(item(&b.doc, "Fillet1")["status"], "warning");
    assert!(!acknowledged(&b.doc, "Fillet1"));

    // Undo of both edits brings the acknowledged state back.
    command(&mut b.doc, json!({"cmd": "undo"}));
    command(&mut b.doc, json!({"cmd": "undo"}));
    assert!(acknowledged(&b.doc, "Fillet1"));

    // A failure: not acknowledged, and forgotten.
    b.doc.kernel().fail.borrow_mut().insert("fillet");
    set_radius(&mut b, 4.0);
    assert_eq!(item(&b.doc, "Fillet1")["status"], "error");
    assert!(!acknowledged(&b.doc, "Fillet1"));
    assert!(!b.doc.to_json().contains("acknowledged_warning"));
}

#[test]
fn errors_and_features_without_a_warning_are_refused() {
    let mut b = warned();
    let error = refused(
        &mut b.doc,
        json!({"cmd": "acknowledge_warnings", "uids": ["F2"]}),
    );
    assert!(
        error.contains("Extrude1 has no warning to acknowledge"),
        "{error}"
    );
    b.doc.kernel().fail.borrow_mut().insert("fillet");
    set_radius(&mut b, 3.0);
    let error = refused(
        &mut b.doc,
        json!({"cmd": "acknowledge_warnings", "uids": ["F3"]}),
    );
    assert!(
        error.contains("Fillet1 failed: an error cannot be acknowledged"),
        "{error}"
    );
    refused(
        &mut b.doc,
        json!({"cmd": "acknowledge_warnings", "uids": ["F99"]}),
    );
    // Showing again what is not acknowledged changes nothing.
    let depth = b.doc.undo_depth();
    command(
        &mut b.doc,
        json!({"cmd": "acknowledge_warnings", "uids": ["F2", "F3"], "acknowledged": false}),
    );
    assert_eq!(b.doc.undo_depth(), depth);
}

#[test]
fn several_warnings_are_acknowledged_in_one_step() {
    let mut b = warned();
    let fillet = json!({"type": "fillet", "body": "F2.b0",
                        "edges": [corner(b.extrude, 1, 2)], "radius": 1});
    command(&mut b.doc, json!({"cmd": "add_feature", "def": fillet}));
    assert_eq!(item(&b.doc, "Fillet2")["status"], "warning");
    let depth = b.doc.undo_depth();
    command(
        &mut b.doc,
        json!({"cmd": "acknowledge_warnings", "uids": ["F3", "F4"]}),
    );
    assert!(acknowledged(&b.doc, "Fillet1"));
    assert!(acknowledged(&b.doc, "Fillet2"));
    assert_eq!(b.doc.undo_depth(), depth + 1);
    assert_eq!(b.doc.undo_label(), Some("Acknowledge 2 Warnings"));
}
