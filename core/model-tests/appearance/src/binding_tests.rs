// SPDX-License-Identifier: MIT
//! Bindings (mitcad#800) with the mock kernel: a body's and a face's
//! appearance switched by a text parameter, a material that follows an
//! expression and brings its appearance (mitcad#640), a feature suppressed
//! by a condition with only the features from it recomputed, an occurrence
//! and a body hidden by a condition, the warning of an unknown appearance
//! name, a value set by hand ending the binding, and the design file, undo,
//! renames and refused deletes.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const SIDE: &str = "F2:side(c1[c4,c2])";

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

fn body(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "bodies"}))[0].clone()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn set_text(doc: &mut Document<MockKernel>, name: &str, text: &str) {
    command(
        doc,
        json!({"cmd": "set_parameter", "name": name, "expression": format!("'{text}'")}),
    );
}

fn add_text(doc: &mut Document<MockKernel>, name: &str, text: &str) {
    command(
        doc,
        json!({"cmd": "add_parameter", "name": name, "expression": format!("'{text}'")}),
    );
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

/// Another body extruded from the block's sketch.
fn another_body(doc: &mut Document<MockKernel>, distance: f64) {
    command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": R}],
               "extent": {"type": "distance", "distance": distance},
               "operation": "new_body"}}),
    );
}

const LOOK: &str = "switch(Color; 'Black'; 'paint_black'; 'Red'; 'Paint - Red'; 'plastic_white')";
const FACE_LOOK: &str = "if(Color = 'Black'; 'paint_white'; 'paint_yellow')";

#[test]
fn a_text_parameter_switches_a_body_and_a_face_appearance() {
    let mut doc = block();
    add_text(&mut doc, "Color", "Black");
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "expression": LOOK}),
    );
    assert_eq!(undo_label(&doc), "Set Appearance of Body1");
    let b = body(&doc);
    assert_eq!(b["appearance"], "paint_black");
    assert_eq!(b["appearance_expr"], LOOK);
    assert_eq!(b["dependencies"], json!(["Color"]));
    assert!(b.get("warnings").is_none(), "{b}");

    // A face follows an expression of its own.
    command(
        &mut doc,
        json!({"cmd": "set_face_appearance", "uid": "F2.b0", "faces": [SIDE],
               "expression": FACE_LOOK}),
    );
    assert_eq!(undo_label(&doc), "Set Appearance of Face of Body1");
    let b = body(&doc);
    assert_eq!(
        b["face_appearances"],
        json!([{"face": SIDE, "appearance": "paint_white", "faces": [SIDE]}])
    );
    assert_eq!(b["face_appearance_exprs"], json!({SIDE: FACE_LOOK}));

    // Changing the parameter switches both; an appearance found by its
    // name is kept by its id.
    set_text(&mut doc, "Color", "Red");
    assert_eq!(undo_label(&doc), "Change Color");
    let b = body(&doc);
    assert_eq!(b["appearance"], "paint_red");
    assert_eq!(b["face_appearances"][0]["appearance"], "paint_yellow");
    set_text(&mut doc, "Color", "Green");
    assert_eq!(body(&doc)["appearance"], "plastic_white");

    // Undo brings back the values with the parameter.
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    let b = body(&doc);
    assert_eq!(b["appearance"], "paint_black");
    assert_eq!(b["face_appearances"][0]["appearance"], "paint_white");

    // An expression that gives a number, or names no parameter, is refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "expression": "2 * 3"}),
    );
    assert!(error.contains("gives a number"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "expression": "Colour"}),
    );
    assert!(error.contains("Colour"), "{error}");
    // A value and an expression together are refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "paint_red",
               "expression": LOOK}),
    );
    assert!(error.contains("both given"), "{error}");
}

#[test]
fn the_material_follows_an_expression_and_brings_its_appearance() {
    let mut doc = block();
    add_text(&mut doc, "Grade", "Plastic");
    // An id, else a material's name (`Steel`).
    let expression = "switch(Grade; 'Plastic'; 'pla'; 'Metal'; 'Steel'; Grade)";
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "expression": expression}),
    );
    assert_eq!(undo_label(&doc), "Set Material of Body1");
    let b = body(&doc);
    assert_eq!(b["material"], "pla");
    assert_eq!(b["material_expr"], expression);
    // The appearance follows the material (mitcad#640).
    assert_eq!(b["appearance"], "plastic_white");
    assert!(b.get("appearance_set_by_hand").is_none(), "{b}");

    set_text(&mut doc, "Grade", "Metal");
    let b = body(&doc);
    assert_eq!(b["material"], "steel");
    assert_eq!(b["appearance"], "steel_satin");

    // An appearance set by hand stays when the material changes.
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "paint_red"}),
    );
    set_text(&mut doc, "Grade", "Plastic");
    let b = body(&doc);
    assert_eq!(b["material"], "pla");
    assert_eq!(b["appearance"], "paint_red");
    assert_eq!(b["material_expr"], expression);

    // A material set by hand ends the binding, in one undo step.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "abs",
               "appearance": "keep"}),
    );
    let b = body(&doc);
    assert_eq!(b["material"], "abs");
    assert!(b.get("material_expr").is_none(), "{b}");
    set_text(&mut doc, "Grade", "Metal");
    assert_eq!(body(&doc)["material"], "abs");
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    let b = body(&doc);
    assert_eq!(b["material"], "pla");
    assert_eq!(b["material_expr"], expression);

    // A name that is no material gives the warning and the component's
    // material.
    set_text(&mut doc, "Grade", "Wood");
    let b = body(&doc);
    assert!(b.get("material").is_none(), "{b}");
    assert_eq!(
        b["warnings"],
        json!(["material: there is no material Wood; the body has its component's material"])
    );
}

#[test]
fn a_condition_suppresses_a_feature_and_only_the_features_from_it_recompute() {
    let mut doc = block();
    another_body(&mut doc, 5.0);
    another_body(&mut doc, 7.0);
    add_text(&mut doc, "Holes", "No");
    command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F3", "expression": "Holes == 'No'"}),
    );
    assert_eq!(undo_label(&doc), "Suppress Extrude2 When");
    let timeline = query(&doc, json!({"query": "timeline"}));
    let feature = &timeline["features"][2];
    assert_eq!(feature["suppressed"], true);
    assert_eq!(feature["suppressed_when"], "Holes == 'No'");
    assert_eq!(feature["dependencies"], json!(["Holes"]));
    let f3 = query(&doc, json!({"query": "feature", "uid": "F3"}));
    assert_eq!(f3["suppressed_when"], "Holes == 'No'");
    assert_eq!(f3["status"], "suppressed");
    // The features before it did not recompute.
    let evaluated = doc.stats().evaluated;
    assert!(!evaluated.contains(&FeatureUid(1)), "{evaluated:?}");
    assert!(!evaluated.contains(&FeatureUid(2)), "{evaluated:?}");
    assert_eq!(
        query(&doc, json!({"query": "bodies"}))
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // It comes back when the condition no longer holds, and nothing before
    // it recomputes. Its own result from before it was suppressed is still
    // in the cache, so it may come back without being evaluated: at most it
    // and the feature after it are.
    set_text(&mut doc, "Holes", "Yes");
    let timeline = query(&doc, json!({"query": "timeline"}));
    assert_eq!(timeline["features"][2]["suppressed"], false);
    let evaluated = doc.stats().evaluated;
    assert!(
        evaluated
            .iter()
            .all(|uid| [FeatureUid(3), FeatureUid(4)].contains(uid)),
        "{evaluated:?}"
    );
    assert_eq!(
        query(&doc, json!({"query": "bodies"}))
            .as_array()
            .unwrap()
            .len(),
        3
    );

    // Saved with the feature and read back.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["features"][2]["suppressed_when"], "Holes == 'No'");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    set_text(&mut loaded, "Holes", "No");
    let timeline = query(&loaded, json!({"query": "timeline"}));
    assert_eq!(timeline["features"][2]["suppressed"], true);

    // Suppressing by hand ends the binding, in one undo step.
    command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F3", "suppressed": false}),
    );
    assert_eq!(undo_label(&doc), "Unsuppress Extrude2");
    assert!(doc.suppressed_when(FeatureUid(3)).is_none());
    set_text(&mut doc, "Holes", "No");
    let timeline = query(&doc, json!({"query": "timeline"}));
    assert_eq!(timeline["features"][2]["suppressed"], false);
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(doc.suppressed_when(FeatureUid(3)), Some("Holes == 'No'"));

    // A condition must give a number.
    let error = refused(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F3", "expression": "Holes"}),
    );
    assert!(error.contains("gives a text"), "{error}");
}

/// A design with a component Board (one 10 x 10 x 5 block) placed once.
fn board() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("Board"), Transform::IDENTITY, true)
        .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    assert_eq!(BodyUid::new(uid, 0).to_string(), "F2.b0");
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc
}

fn occurrence(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "components"}))["occurrences"][0].clone()
}

#[test]
fn an_occurrence_and_a_body_are_hidden_by_conditions() {
    let mut doc = board();
    add_text(&mut doc, "Variant", "Full");
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_visible", "occurrence": "Board:1",
               "expression": "Variant <> 'Bare'"}),
    );
    assert_eq!(undo_label(&doc), "Show Board:1 When");
    let o = occurrence(&doc);
    assert_eq!(o["visible"], true);
    assert_eq!(o["visible_when"], "Variant <> 'Bare'");
    assert_eq!(o["dependencies"], json!(["Variant"]));

    set_text(&mut doc, "Variant", "Bare");
    assert_eq!(occurrence(&doc)["visible"], false);

    // A body's visibility follows a condition too.
    command(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "expression": "Variant == 'Full'"}),
    );
    assert_eq!(undo_label(&doc), "Show Body1 When");
    let b = body(&doc);
    assert_eq!(b["visible"], false);
    assert_eq!(b["visible_when"], "Variant == 'Full'");
    set_text(&mut doc, "Variant", "Full");
    assert_eq!(occurrence(&doc)["visible"], true);
    assert!(body(&doc).get("visible").is_none());

    // Saved and read back.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["occurrences"][0]["visible_when"], "Variant <> 'Bare'");
    assert_eq!(file["bodies"][0]["visible_when"], "Variant == 'Full'");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    set_text(&mut loaded, "Variant", "Bare");
    assert_eq!(occurrence(&loaded)["visible"], false);
    assert_eq!(body(&loaded)["visible"], false);

    // Showing by hand ends the binding, in one undo step.
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_visible", "occurrence": "Board:1", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Hide Board:1");
    assert!(occurrence(&doc).get("visible_when").is_none());
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(occurrence(&doc)["visible_when"], "Variant <> 'Bare'");
    // An empty expression ends the binding and keeps the value.
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_visible", "occurrence": "Board:1", "expression": ""}),
    );
    assert_eq!(undo_label(&doc), "Clear Show When of Board:1");
    let o = occurrence(&doc);
    assert_eq!(o["visible"], true);
    assert!(o.get("visible_when").is_none());
}

#[test]
fn an_unknown_appearance_name_gives_a_warning_and_the_default_look() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "wood_oak"}),
    );
    add_text(&mut doc, "Look", "Gloss Purple");
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "expression": "Look"}),
    );
    let b = body(&doc);
    assert!(b.get("appearance").is_none(), "{b}");
    assert_eq!(
        b["warnings"],
        json!(["appearance: there is no appearance Gloss Purple; the body has the default look"])
    );
    // A name that exists clears the warning.
    set_text(&mut doc, "Look", "Wood - Oak");
    let b = body(&doc);
    assert_eq!(b["appearance"], "wood_oak");
    assert!(b.get("warnings").is_none(), "{b}");
}

#[test]
fn bindings_are_saved_undone_renamed_and_keep_their_parameters() {
    let mut doc = block();
    add_text(&mut doc, "Color", "Black");
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "expression": LOOK}),
    );

    // A rename rewrites the expression.
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "Color", "new_name": "Finish"}),
    );
    let renamed = LOOK.replace("Color", "Finish");
    assert_eq!(body(&doc)["appearance_expr"], renamed);
    assert_eq!(body(&doc)["dependencies"], json!(["Finish"]));
    // The parameter cannot be deleted while the binding uses it.
    let error = refused(
        &mut doc,
        json!({"cmd": "delete_parameter", "name": "Finish"}),
    );
    assert!(error.contains("the appearance of Body1"), "{error}");

    // The design file keeps the expression and the value.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["bodies"][0]["appearance_expr"], renamed);
    assert_eq!(file["bodies"][0]["appearance"], "paint_black");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(body(&loaded)["appearance"], "paint_black");
    set_text(&mut loaded, "Finish", "Red");
    assert_eq!(body(&loaded)["appearance"], "paint_red");

    // An appearance set by hand ends the binding in one undo step.
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "wood_oak"}),
    );
    let b = body(&doc);
    assert_eq!(b["appearance"], "wood_oak");
    assert!(b.get("appearance_expr").is_none(), "{b}");
    command(
        &mut doc,
        json!({"cmd": "delete_parameter", "name": "Finish"}),
    );
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    let b = body(&doc);
    assert_eq!(b["appearance"], "paint_black");
    assert_eq!(b["appearance_expr"], renamed);
    // Undoing the rename gives the old name back.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(body(&doc)["appearance_expr"], LOOK);
}
