// SPDX-License-Identifier: MIT
//! Import notes on timeline items (the model's `document/import_notes.rs`):
//! a note is information with its message (mitcad#1040), kept in the
//! design file and answered in the timeline and by scripts.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::placeholder::BUILDS_NOTHING;
use mitcad_model::test_support::block;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::{DocumentApi, ScriptApi};

const STANDS_IN: &str = "Stands in for Extrude5 (its result is not in the file's history)";

fn item(doc: &Document<MockKernel>, name: &str) -> Value {
    let timeline: Value =
        serde_json::from_str(&doc.query(r#"{"query": "timeline"}"#).unwrap()).unwrap();
    timeline["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no {name} in {timeline}"))
}

#[test]
fn an_import_note_is_information_with_its_message() {
    let mut b = block();
    b.doc
        .set_import_notes(&[(b.extrude, STANDS_IN.to_owned())])
        .unwrap();
    // Information (mitcad#1040): the feature stays ok, its level is info,
    // the note its notice; `error` carries only a failure.
    let extrude = item(&b.doc, "Extrude1");
    assert_eq!(extrude["status"], "ok");
    assert_eq!(extrude["level"], "info");
    assert_eq!(extrude["error"], Value::Null);
    assert_eq!(
        extrude["notices"],
        json!([{"level": "info", "message": STANDS_IN, "target": b.extrude.to_string()}])
    );
    assert_eq!(extrude["import"], STANDS_IN);
    assert_eq!(item(&b.doc, "Sketch1")["status"], "ok");
    assert_eq!(item(&b.doc, "Sketch1")["level"], Value::Null);
    assert!(item(&b.doc, "Sketch1").get("notices").is_none());
    assert!(item(&b.doc, "Sketch1").get("import").is_none());
    let feature: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": b.extrude}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(feature["status"], "ok");
    assert_eq!(feature["level"], "info");
    assert_eq!(feature["notices"], extrude["notices"]);
    assert_eq!(feature["import"], STANDS_IN);
    // Scripts see it as the timeline does.
    let script = json!([{"expect": {"feature": "Extrude1", "status": "ok",
                                    "error_contains": "Stands in for Extrude5"}}]);
    b.doc.run_script(&script.to_string()).unwrap();
    // An undo step; an empty note removes it.
    b.doc.undo();
    assert_eq!(item(&b.doc, "Extrude1")["status"], "ok");
    b.doc.redo();
    b.doc
        .set_import_notes(&[(b.extrude, String::new())])
        .unwrap();
    assert_eq!(item(&b.doc, "Extrude1")["status"], "ok");
    assert!(
        b.doc
            .set_import_notes(&[("F99".parse().unwrap(), "x".to_owned())])
            .is_err()
    );
}

#[test]
fn a_placeholder_is_suppressed_and_builds_nothing() {
    let mut b = block();
    let added = b
        .doc
        .add_import_placeholder(
            Some("Extrude2"),
            "ExtrudeFeature",
            "Suppressed in the file",
            None,
        )
        .unwrap();
    assert_eq!(added.name, "Extrude2");
    let placeholder = item(&b.doc, "Extrude2");
    assert_eq!(placeholder["type"], "placeholder");
    assert_eq!(placeholder["suppressed"], true);
    assert_eq!(placeholder["status"], "suppressed");
    // A state is not a severity: its note is information.
    assert_eq!(placeholder["error"], Value::Null);
    assert_eq!(placeholder["level"], "info");
    assert_eq!(
        placeholder["notices"][0]["message"],
        "Suppressed in the file"
    );
    assert_eq!(b.doc.bodies().len(), 1);
    // Unsuppressed it still builds nothing, and says so: a warning, then
    // the note.
    b.doc.set_suppressed(added.uid, false).unwrap();
    let placeholder = item(&b.doc, "Extrude2");
    assert_eq!(placeholder["status"], "warning");
    assert_eq!(placeholder["level"], "warning");
    assert_eq!(placeholder["error"], Value::Null);
    assert_eq!(
        placeholder["notices"],
        json!([
            {"level": "warning", "message": BUILDS_NOTHING, "target": added.uid.to_string()},
            {"level": "info", "message": "Suppressed in the file",
             "target": added.uid.to_string()}
        ])
    );
    assert_eq!(b.doc.bodies().len(), 1);
    // Features may share a name (mitcad#1107): the item's name is kept;
    // without one it gets the default (the second placeholder's).
    let other = b
        .doc
        .add_import_placeholder(Some("Extrude1"), "", "Failed in the file", None)
        .unwrap();
    assert_eq!(other.name, "Extrude1");
    let unnamed = b
        .doc
        .add_import_placeholder(Some(" "), "", "Failed in the file", None)
        .unwrap();
    assert_eq!(unnamed.name, "Placeholder3");
}

#[test]
fn joins_use_the_join_tolerance_an_importer_set() {
    let mut b = block();
    let join = mitcad_model::test_support::extrude(b.sketch, &[&b.region], 5.0, "join", &[]);
    b.doc.add_feature(&join, None).unwrap();
    // The model's own joins are exact.
    assert_eq!(b.doc.join_tolerance(), None);
    assert_eq!(b.doc.kernel().count("join_within"), 0);
    assert!(b.doc.kernel().count("join") > 0);
    // Set: the join is evaluated again, within the tolerance; the new
    // body is not (it joins nothing).
    let extrusions = b.doc.kernel().count("extrude");
    b.doc.set_join_tolerance(Some(0.01)).unwrap();
    assert_eq!(b.doc.join_tolerance(), Some(0.01));
    assert_eq!(b.doc.kernel().count("join_within"), 1);
    assert_eq!(b.doc.kernel().count("extrude"), extrusions + 1);
    assert!(
        b.doc
            .kernel()
            .specs
            .borrow()
            .contains(&"join within 0.01".to_owned())
    );
    // Unchanged: nothing is evaluated.
    b.doc.set_join_tolerance(Some(0.01)).unwrap();
    assert_eq!(b.doc.kernel().count("join_within"), 1);
    assert!(b.doc.set_join_tolerance(Some(0.0)).is_err());
    assert!(b.doc.set_join_tolerance(Some(f64::NAN)).is_err());
    // The design file keeps it; files without it have none.
    let text = b.doc.to_json();
    let saved: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(saved["join_tolerance"], 0.01);
    let mut reopened = Document::from_json(&text, MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(reopened.join_tolerance(), Some(0.01));
    assert_eq!(reopened.kernel().count("join_within"), 1);
    assert_eq!(reopened.to_json(), text);
    // An undo step takes it back.
    b.doc.undo();
    assert_eq!(b.doc.join_tolerance(), None);
    assert!(
        serde_json::from_str::<Value>(&b.doc.to_json())
            .unwrap()
            .get("join_tolerance")
            .is_none()
    );
    let mut bad = saved;
    bad["join_tolerance"] = json!(-1.0);
    assert!(Document::from_json(&bad.to_string(), MockKernel::default()).is_err());
}

#[test]
fn a_combine_s_join_unites_within_the_join_tolerance() {
    // (mitcad#397) Exact for the model's own designs; within the
    // tolerance an importer set.
    let mut b = block();
    let other = mitcad_model::test_support::extrude(b.sketch, &[&b.region], 5.0, "new_body", &[]);
    let tool = b.doc.add_feature(&other, None).unwrap().uid;
    let combine = mitcad_model::test_support::def(json!({"type": "combine", "target": "F2.b0",
        "tools": [format!("{tool}.b0")], "operation": "join"}));
    b.doc.add_feature(&combine, None).unwrap();
    assert_eq!(b.doc.kernel().count("unite_within"), 0);
    assert_eq!(b.doc.kernel().count("unite"), 1);
    b.doc.set_join_tolerance(Some(0.01)).unwrap();
    assert_eq!(b.doc.kernel().count("unite_within"), 1);
    assert!(
        b.doc
            .kernel()
            .specs
            .borrow()
            .contains(&"unite within 0.01".to_owned())
    );
}

#[test]
fn import_notes_survive_save_and_reopen() {
    let mut b = block();
    b.doc
        .set_import_notes(&[(b.extrude, STANDS_IN.to_owned())])
        .unwrap();
    b.doc
        .add_import_placeholder(
            Some("Fillet2"),
            "FilletFeature",
            "Suppressed in the file",
            None,
        )
        .unwrap();
    let text = b.doc.to_json();
    let saved: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(saved["features"][1]["import_note"], STANDS_IN);
    assert_eq!(saved["features"][2]["type"], "placeholder");
    assert!(saved["features"][0].get("import_note").is_none());
    let mut reopened = Document::from_json(&text, MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(item(&reopened, "Extrude1")["status"], "ok");
    assert_eq!(item(&reopened, "Extrude1")["level"], "info");
    assert_eq!(item(&reopened, "Extrude1")["import"], STANDS_IN);
    assert_eq!(item(&reopened, "Fillet2")["status"], "suppressed");
    assert_eq!(
        item(&reopened, "Fillet2")["notices"][0]["message"],
        "Suppressed in the file"
    );
    assert_eq!(reopened.to_json(), text);
}

#[test]
fn an_edit_that_builds_clears_the_note() {
    let mut b = block();
    b.doc
        .set_import_notes(&[(b.extrude, STANDS_IN.to_owned())])
        .unwrap();
    // Renaming is no edit of the definition.
    b.doc.rename_feature(b.extrude, "Pad").unwrap();
    assert_eq!(item(&b.doc, "Pad")["level"], "info");
    // A dimension that makes it fail keeps the note under the error.
    b.doc.set_parameter("d3", 0.0).unwrap();
    assert_eq!(item(&b.doc, "Pad")["status"], "error");
    assert_eq!(item(&b.doc, "Pad")["level"], "error");
    assert_eq!(item(&b.doc, "Pad")["notices"][0]["level"], "error");
    assert_eq!(item(&b.doc, "Pad")["notices"][1]["level"], "info");
    assert_eq!(b.doc.import_note(b.extrude), Some(STANDS_IN));
    // One that builds clears it; undo brings it back.
    b.doc.set_parameter("d3", 25.0).unwrap();
    assert_eq!(item(&b.doc, "Pad")["status"], "ok");
    assert_eq!(b.doc.import_note(b.extrude), None);
    b.doc.undo();
    assert_eq!(b.doc.import_note(b.extrude), Some(STANDS_IN));
    // An edit of the definition that builds clears it too.
    let def = json!({"type": "extrude",
        "profiles": [{"sketch": b.sketch, "region": b.region}],
        "extent": {"type": "distance", "distance": 30}, "operation": "new_body"});
    b.doc
        .command(&json!({"cmd": "edit_feature", "uid": b.extrude, "def": def}).to_string())
        .unwrap();
    assert_eq!(item(&b.doc, "Pad")["status"], "ok");
    assert_eq!(b.doc.import_note(b.extrude), None);
    // Deleting the feature drops its note.
    b.doc.undo();
    b.doc.delete_feature(b.extrude, false).unwrap();
    assert_eq!(b.doc.import_notes().count(), 0);
}
