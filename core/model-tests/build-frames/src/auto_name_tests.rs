// SPDX-License-Identifier: MIT
//! A BUILD part's automatic name follows its designation (mitcad#2030):
//! a typed length, a second part's ` (1)` name, a name the user gave, a
//! bound length that follows its face, a length formula, a row, a design
//! saved before the record kept the designation; and the `face_run`
//! query's `feature_end` on a profile's free end, which Press Pull offers
//! as the part's length.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_tests_support::test_library::*;

const P30: &str = "Profile 30x30 B8";

fn name_of(doc: &Document<MockKernel>, answer: &Value) -> String {
    doc.assembly().name(component(answer))
}

fn length_of(doc: &Document<MockKernel>, answer: &Value) -> f64 {
    doc.part_info(component(answer)).unwrap().length.unwrap().1
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn edit_length(doc: &mut Document<MockKernel>, answer: &Value, length: f64) {
    command(
        doc,
        json!({"cmd": "build_edit", "component": answer["component"], "length": length}),
    )
    .unwrap_or_else(|e| panic!("build_edit: {e}"));
    all_ok(doc);
}

/// The face an upright's end B is, as a length's `face`.
fn end_b_face(doc: &Document<MockKernel>, answer: &Value) -> Value {
    let part = derived(doc, component(answer));
    json!({"occurrence": answer["occurrence"],
           "geometry": {"body": format!("{part}.b0"), "face": format!("{part}:end_b")}})
}

/// The `face_run` query's `feature_end` on a part's end face.
fn feature_end(doc: &Document<MockKernel>, answer: &Value, end: &str) -> Value {
    let part = derived(doc, component(answer));
    query(
        doc,
        json!({"query": "face_run", "body": format!("{part}.b0"), "face": format!("{part}:{end}")}),
    )["feature_end"]
        .clone()
}

#[test]
fn a_typed_length_renames_the_part_and_one_undo_restores_both() {
    let mut doc = design();
    let p = first(&mut doc, "p30", [0.0; 3], 1000.0);
    all_ok(&doc);
    assert_eq!(name_of(&doc, &p), format!("{P30} L=1000"));
    let record = doc.part_info(component(&p)).unwrap().record;
    assert_eq!(record.named_after, Some(format!("{P30} L=1000")));
    let depth = doc.undo_depth();
    edit_length(&mut doc, &p, 700.0);
    assert_eq!(name_of(&doc, &p), format!("{P30} L=700"));
    assert!(near(length_of(&doc, &p), 700.0));
    // One undo step, labelled as the edit.
    assert_eq!(doc.undo_depth(), depth + 1);
    assert!(
        !doc.undo_label().unwrap_or_default().contains("Rename"),
        "{:?}",
        doc.undo_label()
    );
    let record = doc.part_info(component(&p)).unwrap().record;
    assert_eq!(record.named_after, Some(format!("{P30} L=700")));
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);
    assert_eq!(name_of(&doc, &p), format!("{P30} L=1000"));
    assert!(near(length_of(&doc, &p), 1000.0));
}

#[test]
fn a_second_part_s_suffixed_name_follows_too_made_unique() {
    let mut doc = design();
    let a = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let b = first(&mut doc, "p30", [100.0, 0.0, 0.0], 1000.0);
    let c = first(&mut doc, "p30", [200.0, 0.0, 0.0], 700.0);
    all_ok(&doc);
    assert_eq!(name_of(&doc, &b), format!("{P30} L=1000 (1)"));
    assert_eq!(name_of(&doc, &c), format!("{P30} L=700"));
    // The second at 700: the plain name is the third's, so it takes (1).
    edit_length(&mut doc, &b, 700.0);
    assert_eq!(name_of(&doc, &b), format!("{P30} L=700 (1)"));
    assert_eq!(name_of(&doc, &a), format!("{P30} L=1000"));
    // The first at 500: its plain name.
    edit_length(&mut doc, &a, 500.0);
    assert_eq!(name_of(&doc, &a), format!("{P30} L=500"));
    // The second back at 1000: the plain name is free again.
    edit_length(&mut doc, &b, 1000.0);
    assert_eq!(name_of(&doc, &b), format!("{P30} L=1000"));
}

#[test]
fn a_name_the_user_gave_is_kept_through_a_length_change() {
    let mut doc = design();
    let p = first(&mut doc, "p30", [0.0; 3], 1000.0);
    command(
        &mut doc,
        json!({"cmd": "rename_component", "component": p["component"], "name": "Leg"}),
    )
    .unwrap();
    edit_length(&mut doc, &p, 700.0);
    assert_eq!(name_of(&doc, &p), "Leg");
    assert!(near(length_of(&doc, &p), 700.0));
    // A name given at placement is the user's too.
    let named = place(
        &mut doc,
        json!({"part": part("p30"), "at": {"point": [100, 0, 0]}, "length": 1000, "name": "Rail"}),
    );
    edit_length(&mut doc, &named, 600.0);
    assert_eq!(name_of(&doc, &named), "Rail");
}

#[test]
fn a_bound_length_that_follows_its_face_renames_the_part_in_that_edit_s_step() {
    let mut doc = design();
    let first_upright = first(&mut doc, "p30", [0.0; 3], 500.0);
    let face = end_b_face(&doc, &first_upright);
    // On a base 100 higher, ending level with the first one's top: 400.
    let second = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": {"point": [200, 0, 100]},
               "length": {"mode": "to_face", "face": face, "offset": 0}}),
    );
    all_ok(&doc);
    assert!(near(length_of(&doc, &second), 400.0));
    assert_eq!(name_of(&doc, &second), format!("{P30} L=400"));
    let depth = doc.undo_depth();
    // The first one 600 long: the second follows to 500, renamed in the
    // first one's edit.
    edit_length(&mut doc, &first_upright, 600.0);
    assert_eq!(doc.undo_depth(), depth + 1);
    assert!(near(length_of(&doc, &second), 500.0));
    assert_eq!(name_of(&doc, &first_upright), format!("{P30} L=600"));
    assert_eq!(name_of(&doc, &second), format!("{P30} L=500"));
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);
    assert_eq!(name_of(&doc, &first_upright), format!("{P30} L=500"));
    assert_eq!(name_of(&doc, &second), format!("{P30} L=400"));
}

#[test]
fn a_length_formula_whose_parameter_changes_renames_the_part() {
    let mut doc = design();
    let p = first(&mut doc, "p30", [0.0; 3], 1000.0);
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "Span", "expression": "700 mm"}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_component_parameters", "component": component(&p),
               "parameters": {"L": "Span"}}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(name_of(&doc, &p), format!("{P30} L=700"));
    let depth = doc.undo_depth();
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "Span", "expression": "650 mm"}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(name_of(&doc, &p), format!("{P30} L=650"));
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    assert_eq!(name_of(&doc, &p), format!("{P30} L=700"));
}

#[test]
fn a_row_change_through_build_edit_renames_the_part() {
    let mut doc = design();
    let p = first(&mut doc, "bound-rows", [0.0; 3], 500.0);
    all_ok(&doc);
    let placed_name = name_of(&doc, &p);
    assert!(placed_name.contains("narrow"), "{placed_name}");
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": component(&p), "config": "wide"}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(name_of(&doc, &p), placed_name.replace("narrow", "wide"));
    // The name is the part's designation as the parts list gives it (this
    // source has two bodies: a row per body, the part's designation in its
    // options).
    let rows = query(&doc, json!({"query": "parts_list"}))["rows"].clone();
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["options"]["designation"] == json!(name_of(&doc, &p))),
        "{rows}"
    );
}

/// The design's text without the records' `named_after`, as a design saved
/// before it was kept.
fn without_named_after(text: &str) -> String {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(map) => {
                map.remove("named_after");
                map.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut value: Value = serde_json::from_str(text).unwrap();
    strip(&mut value);
    value.to_string()
}

#[test]
fn a_design_saved_before_follows_after_it_is_opened_and_opening_renames_nothing() {
    let mut doc = design();
    let p = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let q = first(&mut doc, "p30", [100.0, 0.0, 0.0], 1000.0);
    let r = first(&mut doc, "p30", [200.0, 0.0, 0.0], 1000.0);
    command(
        &mut doc,
        json!({"cmd": "rename_component", "component": r["component"], "name": "Post"}),
    )
    .unwrap();
    all_ok(&doc);
    let old_text = without_named_after(&doc.to_json());
    assert!(!old_text.contains("named_after"), "{old_text}");
    let mut opened = Document::from_json(&old_text, MockKernel::default()).unwrap();
    opened.recompute();
    all_ok(&opened);
    // Opening renames nothing and adds nothing to the records.
    assert_eq!(name_of(&opened, &p), format!("{P30} L=1000"));
    assert_eq!(name_of(&opened, &q), format!("{P30} L=1000 (1)"));
    assert_eq!(name_of(&opened, &r), "Post");
    let record = opened.part_info(component(&p)).unwrap().record;
    assert_eq!(record.named_after, None);
    // A name equal to the designation (or with the suffix) follows; the
    // user's stays.
    edit_length(&mut opened, &p, 800.0);
    edit_length(&mut opened, &q, 900.0);
    edit_length(&mut opened, &r, 700.0);
    assert_eq!(name_of(&opened, &p), format!("{P30} L=800"));
    assert_eq!(name_of(&opened, &q), format!("{P30} L=900"));
    assert_eq!(name_of(&opened, &r), "Post");
    let record = opened.part_info(component(&p)).unwrap().record;
    assert_eq!(record.named_after, Some(format!("{P30} L=800")));
}

#[test]
fn face_run_offers_a_profile_s_free_end_as_its_length() {
    let mut doc = design();
    let first_upright = first(&mut doc, "p30", [0.0; 3], 500.0);
    all_ok(&doc);
    // Grounded from end A: end B is the end its length moves.
    let end = feature_end(&doc, &first_upright, "end_b");
    let part = derived(&doc, component(&first_upright));
    assert_eq!(end["uid"], json!(part.to_string()), "{end}");
    assert_eq!(end["name"], json!(name_of(&doc, &first_upright)));
    assert_eq!(end["type"], "derived_part");
    assert_eq!(end["input"], "length");
    // The end it is placed from moves by no length.
    assert_eq!(
        feature_end(&doc, &first_upright, "end_a"),
        Value::Null,
        "the placed end"
    );
    // A length bound to a face: neither end.
    let face = end_b_face(&doc, &first_upright);
    let second = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": {"point": [200, 0, 100]},
               "length": {"mode": "to_face", "face": face, "offset": 0}}),
    );
    all_ok(&doc);
    assert_eq!(
        feature_end(&doc, &second, "end_b"),
        Value::Null,
        "a bound end"
    );
    assert_eq!(feature_end(&doc, &second, "end_a"), Value::Null);
    // The free end's length edit renames the part (Press Pull's Edit
    // Instead sends `build_edit` with `length`).
    edit_length(&mut doc, &first_upright, 650.0);
    assert_eq!(name_of(&doc, &first_upright), format!("{P30} L=650"));
    assert_eq!(
        feature_end(&doc, &first_upright, "end_b")["name"],
        json!(format!("{P30} L=650"))
    );
}
