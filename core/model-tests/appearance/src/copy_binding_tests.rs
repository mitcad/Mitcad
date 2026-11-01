// SPDX-License-Identifier: MIT
//! Editable component copies keep bindings with their copied parameters
//! and face names, while user parameters in the same document stay shared.

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, InsertOptions};
use mitcad_model_api::DocumentApi;

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const SIDE: &str = "F2:side(c1[c4,c2])";
const COPIED_SIDE: &str = "F4:side(c1[c4,c2])";

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    let result = doc
        .command(&value.to_string())
        .unwrap_or_else(|e| panic!("{value}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn body(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "bodies"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|body| body["uid"] == uid)
        .unwrap_or_else(|| panic!("no body {uid}"))
        .clone()
}

fn block(in_component: bool) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    if in_component {
        command(
            &mut doc,
            json!({"cmd": "create_component", "name": "Part", "activate": true}),
        );
    }
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
             "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude",
             "profiles": [{"sketch": "F1", "region": REGION}],
             "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"}
        ]),
    );
    doc
}

fn round_trip(doc: &Document<MockKernel>) -> Document<MockKernel> {
    let saved = doc.to_json();
    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(opened.to_json(), saved);
    opened.recompute();
    opened
}

#[test]
fn paste_new_remaps_body_face_and_suppression_bindings() {
    let mut doc = block(true);
    command(
        &mut doc,
        json!([
            {"cmd": "set_body_material", "uid": "F2.b0",
             "expression": "if(d3 > 10 mm; 'steel'; 'pla')"},
            {"cmd": "set_body_appearance", "uid": "F2.b0",
             "expression": "if(d3 > 10 mm; 'paint_red'; 'paint_black')"},
            {"cmd": "set_face_appearance", "uid": "F2.b0", "faces": [SIDE],
             "expression": "if(d3 > 10 mm; 'paint_red'; 'paint_white')"},
            {"cmd": "set_body_visible", "uid": "F2.b0", "expression": "d3 > 10 mm"},
            {"cmd": "suppress_feature", "uid": "F2", "expression": "d3 > 25 mm"}
        ]),
    );
    let before = doc.to_json();
    command(&mut doc, json!({"cmd": "paste_new", "occurrence": "O1"}));
    let copied = doc.to_json();
    let attributes = doc.body_attributes(BodyUid::new(FeatureUid(4), 0));
    assert_eq!(attributes.visible_when.as_deref(), Some("d6 > 10 mm"));
    assert_eq!(
        attributes.appearance_expr.as_deref(),
        Some("if(d6 > 10 mm; 'paint_red'; 'paint_black')")
    );
    assert_eq!(
        attributes.material_expr.as_deref(),
        Some("if(d6 > 10 mm; 'steel'; 'pla')")
    );
    assert_eq!(
        attributes
            .face_appearance_exprs
            .get(COPIED_SIDE)
            .map(String::as_str),
        Some("if(d6 > 10 mm; 'paint_red'; 'paint_white')")
    );
    assert!(!attributes.face_appearance_exprs.contains_key(SIDE));
    assert_eq!(doc.suppressed_when(FeatureUid(4)), Some("d6 > 25 mm"));
    assert_eq!(body(&doc, "F4.b0")["dependencies"], json!(["d6"]));

    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(doc.to_json(), before);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(doc.to_json(), copied);
    let mut doc = round_trip(&doc);

    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "expression": "20 mm"}),
    );
    let source = body(&doc, "F2.b0");
    let copy = body(&doc, "F4.b0");
    assert_ne!(source["visible"], false);
    assert_eq!(copy["visible"], false);
    assert_eq!(source["appearance"], "paint_red");
    assert_eq!(copy["appearance"], "paint_black");
    assert_eq!(source["material"], "steel");
    assert_eq!(copy["material"], "pla");
    assert_eq!(copy["face_appearances"][0]["appearance"], "paint_white");
    assert_eq!(copy["face_appearances"][0]["face"], COPIED_SIDE);
    assert!(copy.get("warnings").is_none(), "{copy}");

    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d6", "expression": "20 mm"}),
    );
    let copy = body(&doc, "F4.b0");
    assert_ne!(copy["visible"], false);
    assert_eq!(copy["appearance"], "paint_red");
    assert_eq!(copy["material"], "steel");
    assert_eq!(copy["face_appearances"][0]["appearance"], "paint_red");
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "expression": "30 mm"}),
    );
    assert!(doc.feature(FeatureUid(2)).unwrap().suppressed);
    assert!(!doc.feature(FeatureUid(4)).unwrap().suppressed);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d6", "expression": "30 mm"}),
    );
    assert!(doc.feature(FeatureUid(4)).unwrap().suppressed);
}

#[test]
fn paste_new_keeps_binding_only_user_parameters_shared() {
    let mut doc = block(true);
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "HidePart", "expression": "0", "unit": ""},
            {"cmd": "add_parameter", "name": "Color", "expression": "'paint_black'"},
            {"cmd": "suppress_feature", "uid": "F2", "expression": "HidePart == 1"},
            {"cmd": "set_body_appearance", "uid": "F2.b0", "expression": "Color"},
            {"cmd": "paste_new", "occurrence": "O1"}
        ]),
    );
    assert_eq!(doc.suppressed_when(FeatureUid(4)), Some("HidePart == 1"));
    assert_eq!(body(&doc, "F4.b0")["appearance_expr"], "Color");
    let mut doc = round_trip(&doc);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Color", "expression": "'paint_red'"}),
    );
    assert_eq!(body(&doc, "F2.b0")["appearance"], "paint_red");
    assert_eq!(body(&doc, "F4.b0")["appearance"], "paint_red");
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "HidePart", "expression": "1"}),
    );
    assert!(doc.feature(FeatureUid(2)).unwrap().suppressed);
    assert!(doc.feature(FeatureUid(4)).unwrap().suppressed);
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(!doc.feature(FeatureUid(2)).unwrap().suppressed);
    assert!(!doc.feature(FeatureUid(4)).unwrap().suppressed);
    command(&mut doc, json!({"cmd": "redo"}));
    assert!(doc.feature(FeatureUid(4)).unwrap().suppressed);
}

#[test]
fn editable_insert_brings_binding_only_parameters_and_tables() {
    let mut source = block(false);
    command(
        &mut source,
        json!([
            {"cmd": "add_parameter", "name": "HidePart", "expression": "0", "unit": ""},
            {"cmd": "add_parameter", "name": "Color", "expression": "'paint_black'"},
            {"cmd": "set_table", "name": "Flags", "table": {
             "columns": [{"name": "Show", "unit": ""}], "rows": [[1]]}},
            {"cmd": "suppress_feature", "uid": "F2", "expression": "HidePart == 1"},
            {"cmd": "set_body_appearance", "uid": "F2.b0", "expression": "Color"},
            {"cmd": "set_body_visible", "uid": "F2.b0", "expression": "INDEX(Flags.Show; 1) > 0"}
        ]),
    );
    let directory = mitcad_model::testing::unique_temp_dir("copy-bindings");
    let path = directory.join("part.mitcad");
    std::fs::write(&path, source.to_json()).unwrap();
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "HidePart", "expression": "1", "unit": ""},
            {"cmd": "add_parameter", "name": "Color", "expression": "'paint_red'"},
            {"cmd": "set_table", "name": "Flags", "table": {
             "columns": [{"name": "Show", "unit": ""}], "rows": [[0]]}}
        ]),
    );
    doc.insert_component(
        path.to_str().unwrap(),
        &InsertOptions {
            link: false,
            ..InsertOptions::default()
        },
    )
    .unwrap();
    let copy = body(&doc, "F2.b0");
    assert_ne!(copy["visible"], false);
    assert_eq!(copy["appearance"], "paint_black");
    assert_eq!(copy["appearance_expr"], "Color_1");
    assert_eq!(copy["visible_when"], "INDEX(Flags_1.Show; 1) > 0");
    assert_eq!(doc.suppressed_when(FeatureUid(2)), Some("HidePart_1 == 1"));
    assert!(!doc.feature(FeatureUid(2)).unwrap().suppressed);
    assert!(copy.get("warnings").is_none(), "{copy}");
    let mut doc = round_trip(&doc);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "HidePart_1", "expression": "1"}),
    );
    assert!(doc.feature(FeatureUid(2)).unwrap().suppressed);
    std::fs::remove_dir_all(directory).unwrap();
}
