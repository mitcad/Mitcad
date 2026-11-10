// SPDX-License-Identifier: MIT
//! First-use, transactional and dependency behaviour of printed fit parameters.

use serde_json::{Value, json};

use mitcad_model::expr::Unit;
use mitcad_model::features::SketchPlane;
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, FeatureUid, ValueInput};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn rod(diameter: f64) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let circle = doc
        .add_circle(sketch, [0.0, 0.0], &ValueInput::Number(diameter))
        .unwrap();
    doc.add_feature(
        &def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": circle.region}],
        "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}),
        ),
        None,
    )
    .unwrap();
    doc
}

fn thread(extra: Value) -> Value {
    // A coarse pitch keeps a usable crest at the offered flank clearances;
    // a partial thread also leaves its source face for subsequent tests.
    let mut value = json!({"type": "thread", "faces": [{"body": "F2.b0", "face": "F2:side(c1)"}],
        "thread": {"designation": "M30x3.5", "class": "6g"}, "modeled": true, "length": 6});
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    value
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    let uid = doc.add_feature(&def(value), None).unwrap().uid;
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
    uid
}

fn user_parameters(doc: &Document<MockKernel>) -> Value {
    let all = query(doc, json!({"query": "parameters"}));
    Value::Array(
        all.as_array()
            .unwrap()
            .iter()
            .filter(|p| p["kind"] == "user")
            .cloned()
            .collect(),
    )
}

#[test]
fn owner_table_and_virtual_evaluation_do_not_change_a_design() {
    let doc = Document::new(MockKernel::default());
    let before = doc.to_json();
    let fits = query(&doc, json!({"query": "printed_fit_parameters"}));
    let expected = [
        ("thread_radial_clearance", 0.1),
        ("thread_flank_clearance", 0.1),
        ("shaft_bore_clearance", 0.2),
        ("gear_backlash", 0.2),
        ("bearing_radial_clearance", 0.4),
        ("bearing_axial_clearance", 0.4),
        ("bearing_element_gap", 0.6),
        ("bearing_fit_clearance", 0.5),
        ("press_fit_interference", 0.1),
        ("print_in_place_gap", 0.3),
        ("hole_roof_sag", 0.4),
    ];
    assert_eq!(fits.as_array().unwrap().len(), expected.len());
    for (fit, (name, value)) in fits.as_array().unwrap().iter().zip(expected) {
        assert_eq!(fit["name"], name);
        assert_eq!(fit["unit"], "mm");
        assert!(fit["comment"].as_str().unwrap().contains("depends on"));
        let evaluated = query(
            &doc,
            json!({"query": "evaluate", "expression": name, "printed_fits": true}),
        );
        assert!((evaluated["value"].as_f64().unwrap() - value).abs() < 1e-6);
        assert!(
            doc.query(&json!({"query": "evaluate", "expression": name}).to_string())
                .is_err()
        );
    }
    assert_eq!(doc.to_json(), before);
    assert_eq!(doc.undo_depth(), 0);
}

#[test]
fn generator_reports_can_use_fit_names_before_creation() {
    let doc = Document::new(MockKernel::default());
    let before = doc.to_json();
    for (name, definition) in [
        (
            "gear_values",
            json!({"kind": "spur", "plane": "xy", "module": 2,
            "teeth": 20, "face_width": 10, "operation": "new_body", "backlash": "gear_backlash"}),
        ),
        (
            "pulley_values",
            json!({"kind": "toothed", "profile": "gt2_2mm", "plane": "xy",
            "teeth": 40, "belt_width": "shaft_bore_clearance * 30", "operation": "new_body"}),
        ),
        (
            "bearing_values",
            json!({"size": "608", "plane": "xy", "operation": "new_body",
            "radial_clearance": "bearing_radial_clearance", "axial_clearance": "bearing_axial_clearance",
            "element_gap": "bearing_element_gap", "fit_clearance": "bearing_fit_clearance"}),
        ),
    ] {
        let report = query(&doc, json!({"query": name, "def": definition}));
        assert!(report["error"].is_null(), "{name}: {report}");
    }
    assert_eq!(doc.to_json(), before);
}

#[test]
fn thread_first_use_preview_cancel_undo_and_redo_are_atomic() {
    let mut doc = rod(30.0);
    let before = doc.to_json();
    let depth = doc.undo_depth();
    let feature = thread(
        json!({"radial_clearance": "thread_radial_clearance", "flank_clearance": "thread_flank_clearance"}),
    );
    doc.preview_add(&def(feature.clone()), None).unwrap();
    doc.clear_preview();
    assert_eq!(doc.to_json(), before);
    assert_eq!(doc.undo_depth(), depth);
    let uid = add(&mut doc, feature);
    assert_eq!(doc.undo_depth(), depth + 1);
    let params = user_parameters(&doc);
    assert_eq!(params.as_array().unwrap().len(), 2);
    for parameter in params.as_array().unwrap() {
        assert_eq!(parameter["unit"], "mm");
        assert!(parameter["owner"].is_null());
        assert!((parameter["value"].as_f64().unwrap() - 0.1).abs() < 1e-6);
    }
    assert_eq!(
        params[0]["comment"],
        "Printed thread fit, radial; depends on dimensional accuracy"
    );
    assert_eq!(
        params[1]["comment"],
        "Printed thread fit, flanks; depends on overhang quality"
    );
    doc.undo();
    assert_eq!(doc.to_json(), before);
    doc.redo();
    assert_eq!(user_parameters(&doc), params);
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
}

#[test]
fn existing_values_units_comments_and_renamed_ids_win() {
    let mut doc = rod(30.0);
    doc.add_parameter_expression(
        "thread_radial_clearance",
        "0.025 cm",
        Some(Unit::CM),
        "My printer",
    )
    .unwrap();
    let original = user_parameters(&doc);
    let uid = add(
        &mut doc,
        thread(json!({"radial_clearance": "thread_radial_clearance"})),
    );
    assert_eq!(user_parameters(&doc), original);
    assert!(
        doc.kernel()
            .specs
            .borrow()
            .last()
            .unwrap()
            .contains("clearance 0.2500/0.0000")
    );
    doc.rename_parameter("thread_radial_clearance", "printer_thread_gap")
        .unwrap();
    let old_id = doc.parameters().find("printer_thread_gap").unwrap();
    let FeatureStatus::Ok = doc.status(uid).unwrap() else {
        panic!("thread works after rename")
    };
    let renamed = query(&doc, json!({"query": "feature", "uid": uid}));
    assert_eq!(renamed["def"]["radial_clearance"], "printer_thread_gap");
    add(
        &mut doc,
        thread(json!({"radial_clearance": "thread_radial_clearance"})),
    );
    assert_ne!(
        doc.parameters().find("thread_radial_clearance").unwrap(),
        old_id
    );
    assert_eq!(doc.parameters().value(old_id), Some(0.25));
    assert!(doc.delete_parameter("printer_thread_gap").is_err());
}

#[test]
fn changing_a_fit_recomputes_only_its_users() {
    let mut doc = rod(30.0);
    let uid = add(
        &mut doc,
        thread(json!({"flank_clearance": "thread_flank_clearance"})),
    );
    add(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "plane": "xy", "module": 2,
        "teeth": 20, "face_width": 10, "operation": "new_body"}),
    );
    doc.set_parameter("thread_flank_clearance", 0.15).unwrap();
    assert_eq!(doc.stats().evaluated, vec![uid]);
    assert!(
        doc.kernel()
            .specs
            .borrow()
            .last()
            .unwrap()
            .contains("clearance 0.0000/0.1500")
    );
}

#[test]
fn gear_and_pulley_share_a_seated_shaft_fit_and_bearing_creates_four() {
    let mut doc = rod(10.0);
    let seat = json!({"shaft": {"body": "F2.b0", "face": "F2:side(c1)"}});
    add(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "seat": seat, "module": 2,
        "teeth": 20, "face_width": 10, "operation": "new_body",
        "bore_clearance": "shaft_bore_clearance", "backlash": "gear_backlash"}),
    );
    let shared = doc.parameters().find("shaft_bore_clearance").unwrap();
    let pulley = add(
        &mut doc,
        json!({"type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "seat": seat,
        "teeth": 40, "belt_width": 6, "operation": "new_body", "bore_clearance": "shaft_bore_clearance"}),
    );
    assert!(doc.feature(pulley).unwrap().def.params().contains(&shared));
    let before = user_parameters(&doc);
    let bearing = add(
        &mut doc,
        json!({"type": "printable_bearing", "size": "6000", "seat": seat, "operation": "new_body",
        "radial_clearance": "bearing_radial_clearance", "axial_clearance": "bearing_axial_clearance",
        "element_gap": "bearing_element_gap", "fit_clearance": "bearing_fit_clearance"}),
    );
    assert_eq!(user_parameters(&doc).as_array().unwrap().len(), 6);
    assert_eq!(
        doc.feature(bearing)
            .unwrap()
            .def
            .params()
            .iter()
            .filter(|id| doc.parameters().owner(**id).is_none())
            .count(),
        4
    );
    doc.undo();
    assert_eq!(user_parameters(&doc), before);
    doc.redo();
    assert_eq!(user_parameters(&doc).as_array().unwrap().len(), 6);
}

#[test]
fn rejected_add_and_edit_leave_no_fit_parameters() {
    let mut doc = rod(30.0);
    let uid = add(&mut doc, thread(json!({})));
    let before = doc.to_json();
    let depth = doc.undo_depth();
    let bad = thread(json!({"radial_clearance": "thread_radial_clearance + no_such_value"}));
    assert!(doc.add_feature(&def(bad.clone()), None).is_err());
    assert!(doc.edit_feature(uid, &def(bad)).is_err());
    assert_eq!(doc.to_json(), before);
    assert_eq!(doc.undo_depth(), depth);
    // Previewing an edit also makes no persistent user parameter.
    doc.preview_edit(
        uid,
        &def(thread(
            json!({"radial_clearance": "thread_radial_clearance"}),
        )),
    )
    .unwrap();
    doc.clear_preview();
    assert_eq!(doc.to_json(), before);
}

#[test]
fn numeric_omitted_old_loaded_and_imported_definitions_keep_their_values() {
    let mut doc = rod(30.0);
    add(&mut doc, thread(json!({})));
    let uid = add(
        &mut doc,
        thread(json!({"radial_clearance": 0.3, "flank_clearance": "0.05 mm"})),
    );
    assert_eq!(user_parameters(&doc), json!([]));
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    assert_eq!(user_parameters(&loaded), json!([]));
    loaded.recompute();
    assert_eq!(
        loaded.feature(uid).unwrap().def,
        doc.feature(uid).unwrap().def
    );
    // The JSON API still leaves all optional clearances out by default.
    command(
        &mut loaded,
        json!({"cmd": "add_feature", "def": {"type": "printable_bearing",
        "size": "608", "plane": "xy", "operation": "new_body"}}),
    );
    assert_eq!(user_parameters(&loaded), json!([]));
}
