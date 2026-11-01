// SPDX-License-Identifier: MIT
//! Features that share a name (mitcad#1107): a sketch and the extrude made
//! from it may both be named Holder. A place that takes a feature by uid or
//! name refuses the shared name with both uids, unless it takes only one
//! kind and exactly one feature of that kind has the name.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::{DocumentApi, ScriptApi};

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const SHARED: &str = "Holder names several features: F1 (sketch), F2 (extrude); give the uid";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|result| serde_json::from_str(&result).unwrap())
        .map_err(|e| e.to_string())
}

/// Sketch1 (F1) with a 60 x 40 rectangle extruded by Extrude1 (F2), both
/// renamed Holder.
fn holder() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"})).unwrap();
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": REGION}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "rename_feature", "uid": "F1", "name": "Holder"}),
    )
    .unwrap();
    // The extrude takes its sketch's name.
    command(
        &mut doc,
        json!({"cmd": "rename_feature", "uid": "F2", "name": "Holder"}),
    )
    .unwrap();
    doc
}

#[test]
fn an_extrude_renamed_to_its_sketchs_name_keeps_it() {
    let mut doc = holder();
    let names: Vec<(String, String)> = doc
        .features()
        .map(|f| (f.uid.to_string(), f.name.clone()))
        .collect();
    assert_eq!(
        names,
        [
            ("F1".to_owned(), "Holder".to_owned()),
            ("F2".to_owned(), "Holder".to_owned())
        ]
    );
    // An empty name is still refused.
    let error = command(
        &mut doc,
        json!({"cmd": "rename_feature", "uid": "F2", "name": "  "}),
    )
    .unwrap_err();
    assert!(error.contains("empty"), "{error}");
    // A feature added with a name in use keeps it too; default names skip
    // the names in use.
    let added = command(&mut doc, json!({"cmd": "sketch.create", "name": "Holder"})).unwrap();
    assert_eq!(added["name"], "Holder");
    command(
        &mut doc,
        json!({"cmd": "rename_feature", "uid": "F2", "name": "Sketch3"}),
    )
    .unwrap();
    // The third sketch would be Sketch3, which the extrude has now.
    let next = command(&mut doc, json!({"cmd": "sketch.create"})).unwrap();
    assert_eq!(next["name"], "Sketch4");
}

#[test]
fn a_script_naming_a_shared_name_is_refused_with_both_uids() {
    let mut doc = holder();
    let error = doc
        .run_script(&json!([{"expect": {"feature": "Holder", "status": "ok"}}]).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains(SHARED), "{error}");
    // By uid it is one feature.
    doc.run_script(&json!([{"expect": {"feature": "F2", "status": "ok"}}]).to_string())
        .unwrap();
}

#[test]
fn a_sketch_field_with_a_shared_name_finds_the_sketch() {
    let mut doc = holder();
    // A script's sketch expectation takes only sketches.
    doc.run_script(
        &json!([{"expect": {"sketch": "Holder", "regions": [REGION], "dof": 0}}]).to_string(),
    )
    .unwrap();
    // So does export_sketch's sketch.
    let dir = mitcad_model::testing::unique_temp_dir("shared-feature-names");
    let path = dir.join("holder.dxf").to_str().unwrap().to_owned();
    let exported = command(
        &mut doc,
        json!({"cmd": "export_sketch", "sketch": "Holder", "path": path}),
    );
    mitcad_model::testing::remove_temp(&dir);
    assert_eq!(exported.unwrap()["entities"], 4);
}

#[test]
fn joint_commands_naming_a_shared_name_are_refused_with_both_uids() {
    let mut doc = holder();
    // Neither feature of the name is a joint.
    let error = command(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Holder", "values": {}}),
    )
    .unwrap_err();
    assert!(error.contains(SHARED), "{error}");
    let error = command(
        &mut doc,
        json!({"cmd": "add_motion_study", "keys": [{"joint": "Holder", "motion": "rz",
               "points": [[0, 0], [100, 1]]}]}),
    )
    .unwrap_err();
    assert!(error.contains(SHARED), "{error}");
    // A name one feature has is that feature, which is no joint.
    command(
        &mut doc,
        json!({"cmd": "rename_feature", "uid": "F2", "name": "Block"}),
    )
    .unwrap();
    let error = command(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Block", "values": {}}),
    )
    .unwrap_err();
    assert!(error.contains("Block is not a joint"), "{error}");
}

/// A design whose features share a name saves and opens again (an import
/// that keeps the file's repeated names hands its design over this way).
#[test]
fn a_design_with_a_shared_name_opens_again() {
    let doc = holder();
    let opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    let names: Vec<&str> = opened.features().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Holder", "Holder"]);
    assert_eq!(opened.to_json(), doc.to_json());
}
