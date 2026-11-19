// SPDX-License-Identifier: MIT
//! Shots of the Render workspace (mitcad#2013): named cameras in the
//! render settings, added, renamed, moved and deleted as undo steps that
//! recompute nothing; saved in the design file (left out without any),
//! compared between versions, kept by Reset and turned by Reorient.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_diff::diff_states;

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

fn query(doc: &Document<MockKernel>, query: &str) -> Value {
    serde_json::from_str(&doc.query(&json!({"query": query}).to_string()).unwrap()).unwrap()
}

fn shots(doc: &Document<MockKernel>) -> Value {
    query(doc, "render_settings")["shots"].clone()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, "document")["undo"].clone()
}

/// A 20 x 20 x 20 box (F1.b0).
fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 20, "width": 20, "height": 20, "operation": "new_body"}}),
    );
    doc
}

/// The camera the workspace's Save Shot sends.
fn hero() -> Value {
    json!({"cmd": "add_render_shot", "eye": [300, -300, 200], "target": [10, 10, 10],
           "up": [0, 0, 1], "perspective": true, "height": 80})
}

#[test]
fn shots_are_added_renamed_moved_and_deleted_as_undo_steps() {
    let mut doc = block();
    assert_eq!(shots(&doc), json!([]), "a new design lists no shots");
    let features = doc.state().features.clone();
    let revision = query(&doc, "document")["revision"].clone();

    let added = command(&mut doc, hero());
    assert_eq!(added["id"], "shot1");
    assert_eq!(undo_label(&doc), "Add Shot Shot1");
    let shot = &shots(&doc)[0];
    assert_eq!(shot["name"], "Shot1");
    assert_eq!(shot["eye"], json!([300.0, -300.0, 200.0]));
    assert_eq!(shot["target"], json!([10.0, 10.0, 10.0]));
    assert_eq!(shot["perspective"], true);
    assert_eq!(shot["height"], 80.0);
    assert_eq!(doc.state().features, features, "nothing is computed");
    // A shot is saved in the file: the design has a new revision, as a
    // render settings change gives it.
    assert_ne!(query(&doc, "document")["revision"], revision);

    // A name of its own; the next shot takes the next free number.
    let front = command(
        &mut doc,
        json!({"cmd": "add_render_shot", "name": "Front", "eye": [0, -400, 10],
               "target": [0, 0, 10], "perspective": false}),
    );
    assert_eq!(front["id"], "shot2");
    assert_eq!(shots(&doc)[1]["name"], "Front");
    assert_eq!(
        shots(&doc)[1]["up"],
        json!([0.0, 0.0, 1.0]),
        "the default up"
    );
    assert_eq!(shots(&doc)[1]["height"], 200.0, "the default height");

    // Rename and move: one undo step each; the same values are none.
    let renamed = command(
        &mut doc,
        json!({"cmd": "edit_render_shot", "id": "shot1", "name": " Hero "}),
    );
    assert_eq!(renamed["changed"], true);
    assert_eq!(undo_label(&doc), "Rename Shot Shot1 to Hero");
    assert_eq!(shots(&doc)[0]["name"], "Hero", "the name is trimmed");
    command(
        &mut doc,
        json!({"cmd": "edit_render_shot", "id": "shot1", "eye": [250, -250, 150], "height": 120}),
    );
    assert_eq!(undo_label(&doc), "Change Shot Hero");
    assert_eq!(shots(&doc)[0]["height"], 120.0);
    let depth = query(&doc, "document")["undo_depth"].clone();
    let same = command(
        &mut doc,
        json!({"cmd": "edit_render_shot", "id": "shot1", "height": 120}),
    );
    assert_eq!(same["changed"], false);
    assert_eq!(query(&doc, "document")["undo_depth"], depth);

    // Delete, undo, redo; a new shot takes the first free id.
    command(
        &mut doc,
        json!({"cmd": "delete_render_shot", "id": "shot1"}),
    );
    assert_eq!(undo_label(&doc), "Delete Shot Hero");
    assert_eq!(shots(&doc).as_array().unwrap().len(), 1);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(shots(&doc)[0]["name"], "Hero");
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(shots(&doc)[0]["id"], "shot2");
    let again = command(&mut doc, hero());
    assert_eq!(again["id"], "shot1");
    assert_eq!(shots(&doc)[1]["name"], "Shot1");

    // Reset gives the settings their defaults and keeps the shots.
    command(
        &mut doc,
        json!({"cmd": "set_render_settings", "film": {"exposure": 1}}),
    );
    command(&mut doc, json!({"cmd": "reset_render_settings"}));
    assert_eq!(
        query(&doc, "render_settings")["film"]["exposure"],
        0.0,
        "the settings reset"
    );
    assert_eq!(shots(&doc).as_array().unwrap().len(), 2, "the shots stay");
    assert_eq!(doc.state().features, features, "still nothing computed");
}

#[test]
fn wrong_shots_are_refused_and_nothing_changes() {
    let mut doc = block();
    command(&mut doc, hero());
    let before = query(&doc, "render_settings");
    for (given, message) in [
        (
            json!({"cmd": "add_render_shot", "eye": [0, 0, 0], "target": [0, 0, 0]}),
            "the eye must differ from the target",
        ),
        (
            json!({"cmd": "add_render_shot", "eye": [0, 0, 100], "target": [0, 0, 0],
                   "up": [0, 0, 1]}),
            "up must not be along the line of sight",
        ),
        (
            json!({"cmd": "add_render_shot", "height": 0}),
            "height must be positive",
        ),
        (
            json!({"cmd": "add_render_shot", "eye": [1e9, 0, 0]}),
            "must be finite and within",
        ),
        (
            json!({"cmd": "add_render_shot", "name": "  "}),
            "name is empty",
        ),
        (
            json!({"cmd": "add_render_shot", "name": "Shot1"}),
            "named Shot1 already",
        ),
        (json!({"cmd": "add_render_shot", "id": "shot1"}), "exists"),
        (
            json!({"cmd": "add_render_shot", "id": "a b"}),
            "is no shot id",
        ),
        (
            json!({"cmd": "add_render_shot", "zoom": 2}),
            "unknown field `zoom`",
        ),
        (
            json!({"cmd": "edit_render_shot", "id": "shot9", "name": "X"}),
            "there is no shot shot9",
        ),
        (
            json!({"cmd": "edit_render_shot", "id": "shot1", "id2": 1}),
            "unknown field",
        ),
        (
            json!({"cmd": "edit_render_shot", "name": "X"}),
            "needs the shot's id",
        ),
        (
            json!({"cmd": "delete_render_shot", "id": "shot9"}),
            "there is no shot shot9",
        ),
        (
            json!({"cmd": "set_render_settings", "shots": []}),
            "unknown field",
        ),
    ] {
        let error = refused(&mut doc, given.clone());
        assert!(error.contains(message), "{given}: {error}");
    }
    // Two shots cannot share a name by a rename either.
    command(
        &mut doc,
        json!({"cmd": "add_render_shot", "name": "Front", "eye": [0, -400, 0]}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_render_shot", "id": "shot2", "name": "Shot1"}),
    );
    assert!(error.contains("named Shot1 already"), "{error}");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(query(&doc, "render_settings"), before);
}

#[test]
fn shots_are_saved_read_back_compared_and_left_out_without_any() {
    let mut doc = block();
    let plain = doc.to_json();
    let file: Value = serde_json::from_str(&plain).unwrap();
    assert!(
        file.get("render")
            .is_none_or(|render| render.get("shots").is_none()),
        "a design without shots writes none"
    );
    let before = doc.state().clone();
    command(&mut doc, hero());
    command(
        &mut doc,
        json!({"cmd": "add_render_shot", "name": "Top", "eye": [0, 0, 400], "target": [0, 0, 0],
               "up": [0, 1, 0], "perspective": false, "height": 60}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["render"]["shots"][1]["name"], "Top");
    assert_eq!(file["render"]["shots"][1]["perspective"], false);
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    assert_eq!(shots(&loaded), shots(&doc));

    // A shot's fields left out of a file are its defaults; broken ones are
    // refused.
    let mut partial: Value = serde_json::from_str(&plain).unwrap();
    partial["render"] = json!({"shots": [{"id": "s", "name": "S", "eye": [0, -100, 0]}]});
    let partial = Document::from_json(&partial.to_string(), MockKernel::default()).unwrap();
    assert_eq!(shots(&partial)[0]["up"], json!([0.0, 0.0, 1.0]));
    for (broken, expected) in [
        (
            json!({"shots": [{"id": "a", "name": "A", "height": -3}]}),
            "shots.a.height",
        ),
        (
            json!({"shots": [{"id": "a", "name": "A"}, {"id": "a", "name": "B"}]}),
            "two shots a",
        ),
        (
            json!({"shots": [{"id": "a", "name": "A", "lens": 50}]}),
            "unknown field `lens`",
        ),
    ] {
        let mut file: Value = serde_json::from_str(&plain).unwrap();
        file["render"] = broken;
        let Err(error) = Document::from_json(&file.to_string(), MockKernel::default()) else {
            panic!("{} was read", file["render"]);
        };
        assert!(error.to_string().contains(expected), "{error}");
    }

    // The comparison of versions: shots added, removed and changed.
    let diff = diff_states(&before, doc.state());
    let texts: Vec<&str> = diff.document.iter().map(|c| c.text.as_str()).collect();
    assert!(
        texts.contains(&"render.shots.shot1 (none) -> Shot1"),
        "{texts:?}"
    );
    let changed = doc.state().clone();
    command(
        &mut doc,
        json!({"cmd": "edit_render_shot", "id": "shot1", "height": 120}),
    );
    command(
        &mut doc,
        json!({"cmd": "delete_render_shot", "id": "shot2"}),
    );
    let diff = diff_states(&changed, doc.state());
    let texts: Vec<&str> = diff.document.iter().map(|c| c.text.as_str()).collect();
    assert!(
        texts.contains(&"render.shots.shot1.height 80 -> 120"),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"render.shots.shot2 Top -> (none)"),
        "{texts:?}"
    );
}

/// On a design without bodies: the mock kernel's box keeps its axes when
/// its plane turns, so Reorient would refuse the block as not turned
/// exactly.
#[test]
fn reorient_turns_the_shots_with_the_design() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_render_shot", "eye": [0, -500, 0], "target": [0, 0, 0], "up": [0, 0, 1]}),
    );
    command(
        &mut doc,
        json!({"cmd": "reorient_design", "axis": "x", "degrees": 90}),
    );
    let shot = &shots(&doc)[0];
    let eye: Vec<f64> = shot["eye"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let up: Vec<f64> = shot["up"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    // A quarter turn about X takes -Y onto a Z axis and +Z onto a Y axis.
    assert!(eye[0].abs() < 1e-9 && eye[1].abs() < 1e-9, "{eye:?}");
    assert!((eye[2].abs() - 500.0).abs() < 1e-6, "{eye:?}");
    assert!(
        up[2].abs() < 1e-9 && (up[1].abs() - 1.0).abs() < 1e-9,
        "{up:?}"
    );
}
