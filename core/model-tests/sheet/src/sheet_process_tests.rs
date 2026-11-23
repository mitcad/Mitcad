// SPDX-License-Identifier: MIT
//! Sheet process profiles (mitcad#1939; commands.md, "Sheet process
//! profiles") through the JSON api: the profile in the rules, the
//! `sheet_process` query's commands filtered with the reason in words, the
//! allowance methods' parameters and the browser's sheet bodies. The
//! developed lengths with OCCT are `cli.sheet_process_allowances`.

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::FeatureUid;
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn rules(process: Option<&str>, allowance: Value) -> Value {
    let mut rules = json!({"type":"sheet_rules","thickness":3,"bend_radius":0.5,
        "allowance":allowance,"reference_side":"front",
        "bend_relief":{"type":"round","width":1,"depth":2},
        "corner_relief":{"type":"round","width":1,"depth":2}});
    if let Some(process) = process {
        rules["process"] = json!(process);
    }
    rules
}

fn sheet(doc: &mut Document<MockKernel>, rules: FeatureUid) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(40.0), &num(20.0))
        .unwrap()
        .region;
    let face =
        json!({"type":"sheet_face","rules":rules,"profiles":[{"sketch":sketch,"region":region}]});
    doc.add_feature(&def(face), None).unwrap();
}

fn command<'a>(answer: &'a Value, id: &str) -> &'a Value {
    answer["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["command"] == id)
        .unwrap_or_else(|| panic!("{id} in {answer}"))
}

#[test]
fn without_rules_nothing_is_filtered() {
    let doc = Document::new(MockKernel::default());
    let answer = query(&doc, json!({"query":"sheet_process"}));
    assert_eq!(answer["rules"], Value::Null);
    assert_eq!(answer["process"], "metal");
    assert!(
        answer["commands"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["available"] == true),
        "{answer}"
    );
}

#[test]
fn the_rules_profile_filters_the_commands_with_the_reason_in_words() {
    let mut doc = Document::new(MockKernel::default());
    let uid = doc
        .add_feature(
            &def(rules(
                Some("board"),
                json!({"type":"crease_table","rows":[{"thickness":3,"deduction":2}]}),
            )),
            None,
        )
        .unwrap()
        .uid;
    sheet(&mut doc, uid);
    let answer = query(&doc, json!({"query":"sheet_process"}));
    assert_eq!(answer["process"], "board");
    assert_eq!(answer["source"], "rules");
    assert_eq!(answer["label"], "Board");
    assert_eq!(command(&answer, "sheet.glue_flap")["available"], true);
    let finger = command(&answer, "sheet.finger_joint");
    assert_eq!(finger["available"], false);
    let reason = finger["reason"].as_str().unwrap();
    assert!(
        reason.starts_with("Finger joints are for laser-cut sheet"),
        "{reason}"
    );
    let relief = command(&answer, "sheet.corner_relief");
    assert_eq!(relief["available"], false);

    // The browser's sheet bodies say the allowance method and the profile.
    let bodies = query(&doc, json!({"query":"sheet_bodies"}));
    assert_eq!(bodies[0]["process"], "board", "{bodies}");
    assert_eq!(bodies[0]["allowance"], "crease_table");
    assert_eq!(bodies[0]["k_factor"], Value::Null);
}

#[test]
fn laser_sheet_rules_keep_their_kerf_as_a_parameter() {
    let mut doc = Document::new(MockKernel::default());
    let mut def_json = rules(Some("laser_sheet"), json!({"type":"k_factor","k":0.5}));
    def_json["kerf"] = json!(0.15);
    let uid = doc.add_feature(&def(def_json), None).unwrap().uid;
    assert!(matches!(doc.status(uid), Some(FeatureStatus::Ok)));
    let answer = query(&doc, json!({"query":"sheet_rules","feature":uid}));
    assert_eq!(answer["rules"]["kerf"], 0.15, "{answer}");
    assert_eq!(answer["rules"]["process"], "laser_sheet");
    let process = query(&doc, json!({"query":"sheet_process"}));
    assert_eq!(command(&process, "sheet.finger_joint")["available"], true);
    assert_eq!(command(&process, "sheet.glue_flap")["available"], false);
}

#[test]
fn every_allowance_method_is_saved_with_its_parameters() {
    let mut doc = Document::new(MockKernel::default());
    for allowance in [
        json!({"type":"bend_allowance","allowance":4.2}),
        json!({"type":"bend_deduction","deduction":0.1}),
        json!({"type":"crease_table","flute":"C","direction":"across","rows":[
            {"thickness":3.0,"flute":"C","direction":"across","side":"outer","deduction":2.5},
            {"thickness":3.0,"allowance":5.0}]}),
        json!({"type":"kerf_bend","slit_length":20.0,"gap":3.0,"pitch":2.0,"slit_width":0.2}),
    ] {
        let uid = doc
            .add_feature(&def(rules(None, allowance.clone())), None)
            .unwrap()
            .uid;
        assert!(
            matches!(doc.status(uid), Some(FeatureStatus::Ok)),
            "{allowance}: {:?}",
            doc.status(uid)
        );
        let answer = query(&doc, json!({"query":"sheet_rules","feature":uid}));
        assert_eq!(answer["rules"]["allowance"], allowance, "{answer}");
    }
    // A crease table without rows, and a row with both values, fail.
    for bad in [
        json!({"type":"crease_table"}),
        json!({"type":"crease_table","rows":[{"thickness":3,"allowance":5,"deduction":1}]}),
    ] {
        // Refused when added, or failed at its recompute.
        match doc.add_feature(&def(rules(None, bad)), None) {
            Err(_) => {}
            Ok(added) => assert!(matches!(
                doc.status(added.uid),
                Some(FeatureStatus::Failed(_))
            )),
        }
    }
}

/// A material says which profile its sheet parts default to (mitcad#1939):
/// a board material's component has board sheets unless its rules name
/// another; a Wood material gives laser sheet by its class.
#[test]
fn the_material_gives_the_profile_when_the_rules_name_none() {
    let mut doc = Document::new(MockKernel::default());
    let made: Value = serde_json::from_str(
        &doc.command(
            &json!({"cmd":"create_material","name":"Fluted plastic sheet","density":0.9,
                    "sheet_process":"board"})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let id = made["id"].as_str().expect("the material's id").to_owned();
    doc.command(
        &json!({"cmd":"set_component_material","component":"C0","material":id}).to_string(),
    )
    .unwrap();
    let uid = doc
        .add_feature(&def(rules(None, json!({"type":"k_factor","k":0.44}))), None)
        .unwrap()
        .uid;
    sheet(&mut doc, uid);
    let answer = query(&doc, json!({"query":"sheet_process"}));
    assert_eq!(answer["process"], "board", "{answer}");
    assert_eq!(answer["source"], "material");
    // The rules' own profile wins.
    let mut laser = rules(Some("laser_sheet"), json!({"type":"k_factor","k":0.44}));
    laser["kerf"] = json!(0.1);
    doc.add_feature(&def(laser), None).unwrap();
    let answer = query(&doc, json!({"query":"sheet_process"}));
    assert_eq!(answer["process"], "laser_sheet");
    assert_eq!(answer["source"], "rules");
}

/// A standard box's options live in its rules (mitcad#1939): rules of no
/// box are refused by edit_standard_box, in words.
#[test]
fn only_a_standard_boxs_rules_are_edited_as_a_box() {
    let mut doc = Document::new(MockKernel::default());
    let uid = doc
        .add_feature(&def(rules(None, json!({"type":"k_factor","k":0.44}))), None)
        .unwrap()
        .uid;
    let error = doc
        .command(&json!({"cmd":"edit_standard_box","rules":uid,"height":200}).to_string())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("are not the rules of a standard box"),
        "{error}"
    );
    // Options a box cannot have are refused before anything is made.
    let error = doc
        .command(
            &json!({"cmd":"add_standard_box","style":"fefco_0201","length":300,"width":200,
                    "height":150,"board":"no_such_board"})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown board"), "{error}");
}

/// A FEFCO 0421 folder of C flute board with `tabs` locking tabs on each
/// turned over wall: its rules are F1.
fn folder(tabs: u32) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.command(
        &json!({"cmd":"add_standard_box","style":"fefco_0421","length":300,"width":200,
                "height":100,"board":"c_flute","radius":0.5,"tabs":tabs})
        .to_string(),
    )
    .unwrap();
    doc
}

/// The box's features after its rules as the `feature` query gives their
/// definitions: its outline and creases sketches, its Face and its Folds
/// (a Fold's angle, a parameter of its own, left out).
fn box_defs(doc: &Document<MockKernel>) -> Vec<Value> {
    use mitcad_model::features::FeatureDef;
    let uids: Vec<FeatureUid> = doc
        .features()
        .filter(|entry| !matches!(entry.def, FeatureDef::SheetRules(_)))
        .map(|entry| entry.uid)
        .collect();
    uids.into_iter()
        .map(|uid| {
            let mut def = query(doc, json!({"query":"feature","uid":uid}))["def"].take();
            assert!(def.is_object(), "{uid}: {def}");
            if let Some(fields) = def.as_object_mut() {
                fields.remove("angle");
            }
            def
        })
        .collect()
}

/// Another number of locking tabs is the folder's blank of other lines
/// (mitcad#2285): Edit Standard Box makes its outline and creases again,
/// the Face naming the new outline's region and the Folds following, as a
/// box made with those tabs has them, in one undo step that brings the
/// first number back. Another style stays refused in words, the box kept.
#[test]
fn another_number_of_tabs_makes_the_folders_blank_again() {
    let mut doc = folder(3);
    let three = box_defs(&doc);
    let four = box_defs(&folder(4));
    assert_ne!(three, four, "four tabs make another blank");
    let depth = doc.undo_depth();
    let answer: Value = serde_json::from_str(
        &doc.command(&json!({"cmd":"edit_standard_box","rules":"F1","tabs":4}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(answer["face"], "F4", "{answer}");
    assert_eq!(
        answer["folds"].as_array().map(Vec::len),
        Some(14),
        "{answer}"
    );
    assert_eq!(doc.undo_depth(), depth + 1, "one undo step");
    // The Face's region has a new key and keeps its panel slot 0 (its body
    // F4.b0, which the Folds name), no tombstone of the old key beside it.
    let edited = box_defs(&doc);
    let face = &edited[2];
    assert_eq!(face["type"], "sheet_face", "{face}");
    assert_eq!(
        face["panel_slots"],
        json!([{"profile": face["profiles"][0], "slot": 0}]),
        "{face}"
    );
    assert_eq!(edited, four);
    let rules = query(&doc, json!({"query":"feature","uid":"F1"}));
    assert_eq!(rules["def"]["standard_box"]["tabs"], 4, "{rules}");
    // Undo brings the three tabs back.
    doc.undo();
    assert_eq!(doc.undo_depth(), depth);
    assert_eq!(box_defs(&doc), three);
    // Another style makes another box: refused, nothing changed.
    let error = doc
        .command(&json!({"cmd":"edit_standard_box","rules":"F1","style":"fefco_0427"}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("another style"), "{error}");
    assert!(error.contains("make the box again"), "{error}");
    assert_eq!(doc.undo_depth(), depth);
    assert_eq!(box_defs(&doc), three);
}
