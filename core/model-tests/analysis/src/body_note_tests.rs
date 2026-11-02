// SPDX-License-Identifier: MIT
//! Material and process notes (mitcad#1049) on the mock kernel: added on a
//! body's face, their text written from the body's material and its print
//! settings, following a change of either, refused on a datum, and broken
//! when the body goes.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn annotation(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "annotations"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {name}"))
}

fn lines(doc: &Document<MockKernel>, name: &str) -> Value {
    annotation(doc, name)["label"]["lines"].clone()
}

fn material_name(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "body_material", "body": "F2.b0"}))["name"].clone()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

/// The top face of a 20 x 20 x 20 block (F2.b0) on XY.
fn top() -> Value {
    json!({"body": "F2.b0", "face": format!("F2:end({R})")})
}

fn block() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures_selections.set(true);
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "20 mm"}, "operation": "new_body"}},
            {"cmd": "create_print_profile", "kind": "printer", "name": "Bench",
             "values": {"nozzle_diameter": 0.4}},
            {"cmd": "create_print_profile", "kind": "process", "name": "Standard",
             "values": {"layer_height": 0.2, "perimeters": 2}},
            {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
             "value": "bench"},
            {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile",
             "value": "standard"},
        ]),
    );
    doc
}

#[test]
fn a_material_note_writes_the_body_s_material_and_follows_it() {
    let mut doc = block();
    let added = command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "material", "a": top(), "at": [5, 5, 20]}),
    );
    assert_eq!(added["name"], "Material1");
    assert_eq!(added["kind"], "material");
    assert_eq!(undo_label(&doc), "Add Material1");
    let note = annotation(&doc, "Material1");
    assert!(note.get("error").is_none(), "{note}");
    assert_eq!(note["label"]["lines"], json!([material_name(&doc)]));
    assert!(note["drawing"].is_object(), "{note}");
    // The body's own material: the note follows it.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "abs"}),
    );
    let name = material_name(&doc);
    assert_eq!(lines(&doc, "Material1"), json!([name]));
    assert_ne!(name, Value::Null);
    // Its text is the body's, not its own.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_annotation", "name": "Material1", "text": "PLA"}),
    );
    assert!(error.contains("no text of its own"), "{error}");
}

#[test]
fn a_process_note_writes_the_profiles_and_the_values_set_on_the_body() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "process_note", "a": top(), "at": [5, 5, 20]}),
    );
    assert_eq!(
        lines(&doc, "ProcessNote1"),
        json!(["Printer: Bench", "Process: Standard"])
    );
    // A value set on the body is listed; the profile's are not.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "F2.b0", "set": "Print Process",
               "key": "perimeters", "value": 5}),
    );
    assert_eq!(
        lines(&doc, "ProcessNote1"),
        json!(["Printer: Bench", "Process: Standard", "Walls: 5"])
    );
    // Another process profile chosen for the design: the note follows.
    command(
        &mut doc,
        json!([
            {"cmd": "create_print_profile", "kind": "process", "name": "Fine",
             "values": {"layer_height": 0.12}},
            {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile",
             "value": "fine"},
        ]),
    );
    assert_eq!(
        lines(&doc, "ProcessNote1"),
        json!(["Printer: Bench", "Process: Fine", "Walls: 5"])
    );
}

#[test]
fn a_material_or_process_note_points_at_a_body_and_breaks_when_it_goes() {
    let mut doc = block();
    for kind in ["material", "process_note"] {
        let error = refused(
            &mut doc,
            json!({"cmd": "add_annotation", "kind": kind, "a": {"datum": "xy"}}),
        );
        assert!(error.contains("points at a body"), "{error}");
    }
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "material", "a": top(), "at": [5, 5, 20]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_annotation", "kind": "process_note", "a": top(), "at": [5, 5, 20]}),
    );
    // The body gone (its feature deleted): both broken, with the reason.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    for name in ["Material1", "ProcessNote1"] {
        let broken = annotation(&doc, name);
        assert!(
            broken["error"]
                .as_str()
                .is_some_and(|e| e.starts_with("a pick is not there")),
            "{broken}"
        );
        assert!(broken.get("drawing").is_none(), "{broken}");
    }
}

#[test]
fn the_bodies_whose_material_or_print_settings_differ_are_listed() {
    let mut doc = block();
    let differences = |doc: &Document<MockKernel>| query(doc, json!({"query": "make_differences"}));
    // Nothing of its own: the body prints as its component does.
    assert_eq!(differences(&doc), json!([]));
    // A value set on the body, and a material of its own.
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "F2.b0", "set": "Print Process",
               "key": "perimeters", "value": 5}),
    );
    let listed = differences(&doc);
    assert_eq!(listed[0]["body"], "F2.b0", "{listed}");
    assert_eq!(listed[0]["material"], Value::Null, "{listed}");
    assert_eq!(listed[0]["print"], json!(["Walls"]), "{listed}");
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "abs"}),
    );
    assert_eq!(differences(&doc)[0]["material"], material_name(&doc));
    // The component's default the same as the body's own: no difference
    // in material.
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Root", "material": "abs"}),
    );
    assert_eq!(differences(&doc)[0]["material"], Value::Null);
    // The value taken away: nothing differs any more.
    command(
        &mut doc,
        json!({"cmd": "remove_property", "owner": "F2.b0", "set": "Print Process",
               "key": "perimeters"}),
    );
    assert_eq!(differences(&doc), json!([]));
}

/// Root › Arm › Pin, only Arm with a default material (mitcad#1150): a
/// body of Pin made of Arm's material is made of what its components give
/// it, so it differs in nothing; another material of its own differs.
#[test]
fn a_material_given_by_a_component_further_up_is_no_difference() {
    use mitcad_model::ids::ComponentUid;
    use mitcad_model::transform::Transform;
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("Arm"), Transform::IDENTITY, true)
        .unwrap();
    doc.create_component(Some("Pin"), Transform::translation([5.0, 0.0, 0.0]), true)
        .unwrap();
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 4, "width": 4, "height": 4, "operation": "new_body"}}),
    );
    let body = format!("{}.b0", added["uid"].as_str().unwrap());
    doc.activate_component(ComponentUid::ROOT).unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Arm", "material": "abs"}),
    );
    let differences = |doc: &Document<MockKernel>| query(doc, json!({"query": "make_differences"}));
    assert_eq!(differences(&doc), json!([]));
    // Its own material the same as Arm's: as its note says, like its parent.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": body, "material": "abs"}),
    );
    assert_eq!(
        query(&doc, json!({"query": "body_material", "body": body}))["source"],
        "body"
    );
    assert_eq!(differences(&doc), json!([]));
    // Another material of its own differs.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": body, "material": "pla"}),
    );
    let listed = differences(&doc);
    assert_eq!(listed.as_array().map(Vec::len), Some(1), "{listed}");
    assert_eq!(listed[0]["body"], body, "{listed}");
    assert_ne!(listed[0]["material"], Value::Null, "{listed}");
}
