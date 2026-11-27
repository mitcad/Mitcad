// SPDX-License-Identifier: MIT
//! Sheet process profiles (mitcad#1939; commands.md, "Sheet process
//! profiles") through the JSON api: the profile in the rules, the
//! `sheet_process` query's commands filtered with the reason in words, the
//! allowance methods' parameters and the browser's sheet bodies. The
//! developed lengths with OCCT are `cli.sheet_process_allowances`.

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
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

/// A flat FEFCO 0421 folder (Folded off: no Folds) of C flute board,
/// `height` high with `tabs` locking tabs: its rules are F1, its outline
/// F2, its creases F3 and its Face F4.
fn flat_folder(tabs: u32, height: f64) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.command(
        &json!({"cmd":"add_standard_box","style":"fefco_0421","length":300,"width":200,
                "height":height,"board":"c_flute","radius":0.5,"tabs":tabs,
                "folded":false})
        .to_string(),
    )
    .unwrap();
    doc
}

/// A flat box has no Folds to find its creases sketch by (mitcad#2301):
/// Edit Standard Box makes that sketch again too, so the flat blank's
/// outline and crease lines are the new box's, the box stays flat, and
/// undo brings the old ones back. Another number of tabs gives the
/// outline other slots on the same creases; another height moves the
/// creases.
#[test]
fn a_flat_boxs_creases_follow_its_edit() {
    let three = box_defs(&flat_folder(3, 100.0));
    assert_eq!(three.len(), 3, "outline, creases and Face, no Folds");
    assert_eq!(three[1]["type"], "sketch", "{}", three[1]);
    for (edit, expected, other_creases) in [
        (json!({"tabs": 4}), flat_folder(4, 100.0), false),
        (json!({"height": 120}), flat_folder(3, 120.0), true),
    ] {
        let expected = box_defs(&expected);
        assert_ne!(three, expected, "{edit}: the new box has another blank");
        assert_eq!(
            three[1]["entities"] != expected[1]["entities"],
            other_creases,
            "{edit}: the new box's creases"
        );
        let mut doc = flat_folder(3, 100.0);
        let depth = doc.undo_depth();
        let mut command = json!({"cmd":"edit_standard_box","rules":"F1"});
        for (key, value) in edit.as_object().unwrap() {
            command[key] = value.clone();
        }
        let answer: Value =
            serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap();
        assert_eq!(answer["creases"], "F3", "{edit}: {answer}");
        assert_eq!(answer["folds"], json!([]), "{edit}: {answer}");
        assert_eq!(doc.undo_depth(), depth + 1, "{edit}: one undo step");
        assert_eq!(box_defs(&doc), expected, "{edit}");
        doc.undo();
        assert_eq!(doc.undo_depth(), depth, "{edit}");
        assert_eq!(
            box_defs(&doc),
            three,
            "{edit}: undo brings the old creases back"
        );
    }
}

/// Edit Standard Box changes the box's rules, outline, creases (and, for
/// another number of tabs, its Folds) in one recompute (mitcad#2495: a
/// recompute for each of them built a folder's eighteen Folds again each
/// time), and the box is the one made with the new options.
#[test]
fn a_box_edit_recomputes_once() {
    for (edit, expected) in [
        (json!({"height": 120}), json!({"height": 120, "tabs": 3})),
        (json!({"tabs": 4}), json!({"height": 100, "tabs": 4})),
    ] {
        let mut doc = folder(3);
        let before = doc.recompute_count();
        let mut command = json!({"cmd":"edit_standard_box","rules":"F1"});
        for (key, value) in edit.as_object().unwrap() {
            command[key] = value.clone();
        }
        doc.command(&command.to_string()).unwrap();
        // The rules, sketches, Face and kept Folds in one recompute: a
        // recompute for each made 3 (another height) and 16 (twelve Folds
        // edited for four tabs). Another number of tabs then adds the two
        // new creases' Folds after the box's last, each its own command
        // (at most a recompute each).
        let recomputes = doc.recompute_count() - before;
        if edit.get("tabs").is_some() {
            assert!(
                (1..=3).contains(&recomputes),
                "{edit}: the box's features edited in one recompute, got {recomputes}"
            );
        } else {
            assert_eq!(
                recomputes, 1,
                "{edit}: the box's features edited in one recompute"
            );
        }
        let mut made = Document::new(MockKernel::default());
        made.command(
            &json!({"cmd":"add_standard_box","style":"fefco_0421","length":300,"width":200,
                    "height":expected["height"],"board":"c_flute","radius":0.5,
                    "tabs":expected["tabs"]})
            .to_string(),
        )
        .unwrap();
        assert_eq!(box_defs(&doc), box_defs(&made), "{edit}");
    }
}

/// What a refused command leaves as it was: the design, the undo depth and
/// the labels of Undo and Redo.
fn history(doc: &Document<MockKernel>) -> (String, usize, Option<String>, Option<String>) {
    (
        doc.to_json(),
        doc.undo_depth(),
        doc.undo_label().map(str::to_owned),
        doc.redo_label().map(str::to_owned),
    )
}

/// A glue flap its inset leaves no room for is refused and leaves nothing
/// (mitcad#2527): not the profile's plane and sketch it made first, nor
/// their undo step, and the redo step before it stays. With room the flap
/// is made, one undo step.
#[test]
fn a_refused_glue_flap_leaves_the_design_and_its_steps_as_they_were() {
    let mut doc = Document::new(MockKernel::default());
    let rules = doc
        .add_feature(&def(rules(None, json!({"type":"k_factor","k":0.4}))), None)
        .unwrap()
        .uid;
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(40.0), &num(20.0))
        .unwrap()
        .region;
    let face =
        json!({"type":"sheet_face","rules":rules,"profiles":[{"sketch":sketch,"region":region}]});
    let face = doc.add_feature(&def(face), None).unwrap().uid;
    let body = BodyUid::new(face, 0);
    // A flange of 15 mm on the Face's edge x = 40, 20 mm long.
    let flange = json!({"type":"sheet_flange","body":body,
        "boundary":{"panel":{"feature":face,"slot":0},
            "curve":{"type":"sketch","sketch":sketch,"segment":"c2[c1,c3]"}},
        "angle":std::f64::consts::FRAC_PI_2,"length":{"mode":"tangent","value":15},
        "position":"inside"});
    // A redo step of the person's own.
    doc.set_suppressed(face, true).unwrap();
    doc.undo();
    let before = history(&doc);
    assert!(before.3.is_some(), "a redo step");
    let error = doc
        .command(&json!({"cmd":"add_glue_flap","def":flange,"inset":12}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("leave no flap"), "{error}");
    assert_eq!(history(&doc), before);
    doc.command(&json!({"cmd":"add_glue_flap","def":flange,"inset":2}).to_string())
        .unwrap();
    assert_eq!(doc.undo_depth(), before.1 + 1);
    assert_eq!(doc.undo_label(), Some("Glue Flap"));
    assert_eq!(doc.redo_label(), None);
}

/// A refused Edit Standard Box leaves no half-made edit to redo
/// (mitcad#2527): what it changed before the refusal goes, the undo depth
/// and the redo step before it as they were. Here a Fold on the box's
/// creases sketch past its creases, which the box takes as its own, is
/// used by an Unfold, so the edit cannot delete it, after it changed the
/// rules.
#[test]
fn a_refused_box_edit_leaves_no_redo_step() {
    let mut doc = folder(3);
    let fold = doc
        .add_feature(
            &def(json!({"type":"sheet_fold","body":"F4.b0",
                "panel":{"feature":"F4","slot":0},"line":{"sketch":"F3","curve":"c300"},
                "stationary_side":"left","angle":1.0,"position":"bend_line"})),
            None,
        )
        .unwrap()
        .uid;
    let unfold = doc
        .add_feature(
            &def(json!({"type":"sheet_unfold","body":"F4.b0",
                "stationary":{"feature":"F4","slot":0},"bends":[{"feature":fold,"slot":0}]})),
            None,
        )
        .unwrap()
        .uid;
    // A redo step of the person's own.
    doc.set_suppressed(unfold, true).unwrap();
    doc.undo();
    let before = history(&doc);
    assert!(before.3.is_some(), "a redo step");
    let error = doc
        .command(&json!({"cmd":"edit_standard_box","rules":"F1","tabs":4}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("no crease for the box's Fold"), "{error}");
    assert_eq!(history(&doc), before);
    assert_ne!(doc.redo_label(), Some("Edit Standard Box"));
}
