// SPDX-License-Identifier: MIT
//! Extrude's automatic operation (mitcad#289), with the mock kernel, whose
//! contact test works on bounding boxes: a preview asked for it reports
//! Join for a tool on a body's face extruded away from it, Cut for one
//! extruded into it and New Body for one that touches no visible body of
//! its component; bodies whose boxes the tool's does not reach never get
//! the kernel's contact test.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, RegionKey};
use mitcad_model_api::DocumentApi;

/// A 10 x 10 block 20 deep under the XY plane: its top face lies in XY.
fn block(doc: &mut Document<MockKernel>) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], -20.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

/// A 6 x 6 square on XY at `corner`, not used yet.
fn square(doc: &mut Document<MockKernel>, corner: [f64; 2]) -> (FeatureUid, RegionKey) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, corner, &num(6.0), &num(6.0))
        .unwrap()
        .region;
    (sketch, region)
}

/// The preview of extruding the square `distance` with `operation`, the
/// automatic operation asked for or not.
fn preview(
    doc: &mut Document<MockKernel>,
    (sketch, region): &(FeatureUid, RegionKey),
    distance: f64,
    operation: &str,
    auto: bool,
) -> Value {
    let def = extrude(*sketch, &[region], distance, operation, &[]);
    let mut command = json!({"cmd": "add_feature", "def": def});
    if auto {
        command["auto_operation"] = json!(true);
    }
    let report: Value = serde_json::from_str(&doc.preview(&command.to_string()).unwrap()).unwrap();
    assert_eq!(report["status"], "ok", "{report}");
    report
}

#[test]
fn a_profile_on_a_face_joins_going_out_and_cuts_going_in() {
    let mut doc = Document::new(MockKernel::default());
    let body = block(&mut doc);
    let square = square(&mut doc, [2.0, 2.0]);

    let out = preview(&mut doc, &square, 5.0, "new_body", true);
    assert_eq!(
        out["auto_operation"],
        json!({"operation": "join", "objects": [body.to_string()]}),
        "{out}"
    );
    let into = preview(&mut doc, &square, -5.0, "new_body", true);
    assert_eq!(
        into["auto_operation"],
        json!({"operation": "cut", "objects": [body.to_string()]}),
        "{into}"
    );
    // Back out again: Join.
    let again = preview(&mut doc, &square, 3.0, "cut", true);
    assert_eq!(again["auto_operation"]["operation"], "join", "{again}");
}

#[test]
fn the_report_is_the_definitions_own() {
    let mut doc = Document::new(MockKernel::default());
    let body = block(&mut doc);
    let square = square(&mut doc, [2.0, 2.0]);
    // Asked as a cut while the tool goes out: the cut is what is previewed
    // (the body changed), and the choice says Join.
    let report = preview(&mut doc, &square, 5.0, "cut", true);
    assert_eq!(report["auto_operation"]["operation"], "join", "{report}");
    let changed = report["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == body.to_string())
        .map(|b| b["changed"].clone());
    assert_eq!(changed, Some(json!(true)), "{report}");
    // As a new body the block stays as it is.
    let plain = preview(&mut doc, &square, 5.0, "new_body", true);
    let changed = plain["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == body.to_string())
        .map(|b| b["changed"].clone());
    assert_eq!(changed, Some(json!(false)), "{plain}");
}

#[test]
fn a_profile_away_from_bodies_is_a_new_body_without_contact_tests() {
    let mut doc = Document::new(MockKernel::default());
    block(&mut doc);
    let square = square(&mut doc, [30.0, 30.0]);
    let report = preview(&mut doc, &square, -5.0, "new_body", true);
    assert_eq!(
        report["auto_operation"],
        json!({"operation": "new_body", "objects": []}),
        "{report}"
    );
    // The boxes are apart: the kernel's contact test is not asked.
    assert_eq!(doc.kernel().count("tool_contacts"), 0);
    // An empty design: New Body.
    let mut empty = Document::new(MockKernel::default());
    let square = self::square(&mut empty, [0.0, 0.0]);
    let report = preview(&mut empty, &square, 5.0, "new_body", true);
    assert_eq!(
        report["auto_operation"]["operation"], "new_body",
        "{report}"
    );
}

#[test]
fn hidden_bodies_and_other_components_bodies_are_not_touched() {
    let mut doc = Document::new(MockKernel::default());
    let body = block(&mut doc);
    let square = square(&mut doc, [2.0, 2.0]);
    doc.set_body_visible(body, false).unwrap();
    let report = preview(&mut doc, &square, -5.0, "new_body", true);
    assert_eq!(
        report["auto_operation"]["operation"], "new_body",
        "{report}"
    );
    doc.set_body_visible(body, true).unwrap();

    // A component of its own, placed where the root's block is: its
    // features work on its own bodies only.
    doc.create_component(
        Some("Part"),
        mitcad_model::transform::Transform::translation([0.0, 0.0, 0.0]),
        true,
    )
    .unwrap();
    let inner = self::square(&mut doc, [2.0, 2.0]);
    let report = preview(&mut doc, &inner, -5.0, "new_body", true);
    assert_eq!(
        report["auto_operation"]["operation"], "new_body",
        "{report}"
    );
}

/// A failed preview of the definition still carries the suggestion
/// (mitcad#2216): the app tries the suggested operation after a direction
/// flip whose preview failed with the user's own.
#[test]
fn a_failed_definition_still_reports_the_suggestion() {
    let mut doc = Document::new(MockKernel::default());
    block(&mut doc);
    let far = square(&mut doc, [30.0, 30.0]);
    // The mock's Booleans reach no target, as the real ones away from the
    // block.
    *doc.kernel().touch.borrow_mut() = Some(vec![false]);
    for operation in ["cut", "join"] {
        let def = extrude(far.0, &[&far.1], 5.0, operation, &[]);
        let command = json!({"cmd": "add_feature", "def": def, "auto_operation": true});
        let report: Value =
            serde_json::from_str(&doc.preview(&command.to_string()).unwrap()).unwrap();
        assert_ne!(
            report["status"], "ok",
            "a {operation} that reaches no body fails: {report}"
        );
        assert_eq!(
            report["auto_operation"],
            json!({"operation": "new_body", "objects": []}),
            "{report}"
        );
    }
}

#[test]
fn without_the_request_nothing_is_chosen() {
    let mut doc = Document::new(MockKernel::default());
    block(&mut doc);
    let square = square(&mut doc, [2.0, 2.0]);
    let report = preview(&mut doc, &square, 5.0, "new_body", false);
    assert!(report.get("auto_operation").is_none(), "{report}");
    assert_eq!(doc.kernel().count("tool_contacts"), 0);
}

/// Edit previews opt in through the same API field as additions. The
/// committed state, marker, results and undo history must stay untouched.
fn edit_preview(
    doc: &mut Document<MockKernel>,
    uid: FeatureUid,
    profile: &(FeatureUid, RegionKey),
    distance: f64,
    operation: &str,
    auto: Option<bool>,
) -> Value {
    let def = extrude(profile.0, &[&profile.1], distance, operation, &[]);
    let mut command = json!({"cmd":"edit_feature", "uid":uid, "def":def});
    if let Some(auto) = auto {
        command["auto_operation"] = json!(auto);
    }
    serde_json::from_str(&doc.preview(&command.to_string()).unwrap()).unwrap()
}

fn unchanged_state(
    doc: &Document<MockKernel>,
) -> (String, u64, usize, Option<String>, Option<String>, u64) {
    (
        doc.to_json(),
        doc.revision(),
        doc.undo_depth(),
        doc.undo_label().map(str::to_owned),
        doc.redo_label().map(str::to_owned),
        doc.recompute_count(),
    )
}

#[test]
fn an_edit_reports_into_out_and_away_without_changing_the_definition_or_undo() {
    let mut doc = Document::new(MockKernel::default());
    let body = block(&mut doc);
    let profile = square(&mut doc, [2.0, 2.0]);
    let away = square(&mut doc, [30.0, 30.0]);
    let source = doc
        .add_feature(
            &extrude(profile.0, &[&profile.1], 5.0, "join", &[body]),
            None,
        )
        .unwrap()
        .uid;
    let original = unchanged_state(&doc);
    let original_shape = doc.body_shape(body).unwrap().clone();
    for (profile, distance, expected, objects) in [
        (&profile, -5.0, "cut", vec![body]),
        (&profile, 5.0, "join", vec![body]),
        (&away, 5.0, "new_body", vec![]),
    ] {
        // A suggestion never substitutes its operation for the definition
        // supplied in the same request, even if that definition is New Body.
        let report = edit_preview(&mut doc, source, profile, distance, "new_body", Some(true));
        assert_eq!(report["status"], "ok", "{report}");
        assert_eq!(
            report["auto_operation"],
            json!({"operation":expected,"objects":objects}),
            "{report}"
        );
        let mitcad_model::features::FeatureDef::Extrude(previewed) =
            &doc.preview_feature().unwrap().def
        else {
            panic!("expected an extrusion preview");
        };
        assert_eq!(
            previewed.operation,
            mitcad_model::features::Operation::NewBody
        );
        assert_eq!(unchanged_state(&doc), original);
        assert_eq!(doc.body_shape(body), Some(&original_shape));
    }
    // A suggestion is transient: clearing it leaves the original Join and
    // its ordinary undo step in place.
    doc.clear_preview();
    assert_eq!(unchanged_state(&doc), original);
    doc.undo().unwrap();
    assert!(doc.features().all(|entry| entry.uid != source));
    doc.redo().unwrap();
    assert_eq!(doc.body_shape(body), Some(&original_shape));
}

#[test]
fn edit_auto_operation_is_optional_and_other_features_report_no_choice() {
    let mut doc = Document::new(MockKernel::default());
    let target = block(&mut doc);
    let profile = square(&mut doc, [2.0, 2.0]);
    let source = doc
        .add_feature(
            &extrude(profile.0, &[&profile.1], 5.0, "join", &[target]),
            None,
        )
        .unwrap()
        .uid;
    for auto in [None, Some(false)] {
        let report = edit_preview(&mut doc, source, &profile, -5.0, "new_body", auto);
        assert_eq!(report["status"], "ok", "{report}");
        assert!(report.get("auto_operation").is_none(), "{report}");
    }
    assert_eq!(doc.kernel().count("tool_contacts"), 0);
    let actual_edit = json!({"cmd":"edit_feature", "uid":source, "auto_operation":true,
        "def":extrude(profile.0, &[&profile.1], -5.0, "new_body", &[])});
    assert!(
        doc.command(&actual_edit.to_string())
            .unwrap_err()
            .0
            .contains("auto_operation")
    );
    let unrelated = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let sketch: mitcad_model::features::FeatureDef<mitcad_model::features::ValueInput> =
        serde_json::from_value(json!({"type":"sketch","plane":"xy"})).unwrap();
    let command = json!({"cmd":"edit_feature","uid":unrelated,"def":sketch,"auto_operation":true});
    let report: Value = serde_json::from_str(&doc.preview(&command.to_string()).unwrap()).unwrap();
    assert!(report.get("auto_operation").is_none(), "{report}");
    assert_eq!(doc.kernel().count("tool_contacts"), 0);
}

#[test]
fn edited_feature_and_later_bodies_are_never_candidates_at_any_marker() {
    let mut doc = Document::new(MockKernel::default());
    let profile = square(&mut doc, [2.0, 2.0]);
    let source = doc
        .add_feature(
            &extrude(profile.0, &[&profile.1], 5.0, "new_body", &[]),
            None,
        )
        .unwrap()
        .uid;
    let own = BodyUid::new(source, 0);
    let later = block(&mut doc);
    let position = doc
        .state()
        .features
        .iter()
        .position(|entry| entry.uid == source)
        .unwrap();
    let end = doc.state().features.len();
    for marker in [0, position, position + 1, end] {
        doc.set_marker(marker).unwrap();
        let original = unchanged_state(&doc);
        for distance in [-5.0, 5.0] {
            let report = edit_preview(&mut doc, source, &profile, distance, "new_body", Some(true));
            assert_eq!(
                report["auto_operation"],
                json!({"operation":"new_body","objects":[]}),
                "own {own}, later {later}, marker {marker}: {report}"
            );
            assert_eq!(unchanged_state(&doc), original);
        }
    }
    assert_eq!(doc.kernel().count("tool_contacts"), 0);
}

#[test]
fn edit_targets_use_pre_feature_shapes_even_when_the_final_shape_overlaps() {
    let mut doc = Document::new(MockKernel::default());
    let body = block(&mut doc);
    let profile = square(&mut doc, [2.0, 2.0]);
    let source = doc
        .add_feature(
            &extrude(profile.0, &[&profile.1], 5.0, "join", &[body]),
            None,
        )
        .unwrap()
        .uid;
    let before = doc.state().features.len() - 1;
    doc.add_feature(
        &extrude(profile.0, &[&profile.1], 20.0, "join", &[body]),
        None,
    )
    .unwrap();
    let end = doc.state().features.len();
    for marker in [0, before, before + 1, end] {
        doc.set_marker(marker).unwrap();
        for distance in [3.0, 12.0] {
            let report = edit_preview(&mut doc, source, &profile, distance, "new_body", Some(true));
            // The original Join encloses the shorter tool and the later
            // Join encloses the longer one; either could incorrectly
            // suggest Cut. The source's input ends at XY and suggests Join.
            assert_eq!(
                report["auto_operation"],
                json!({"operation":"join","objects":[body]}),
                "marker {marker}, distance {distance}: {report}"
            );
        }
    }
}

#[test]
fn edit_targets_follow_visibility_and_owning_component_not_the_active_component() {
    let mut doc = Document::new(MockKernel::default());
    let root = block(&mut doc);
    doc.create_component(Some("Part"), mitcad_model::Transform::IDENTITY, true)
        .unwrap();
    let local = block(&mut doc);
    let profile = square(&mut doc, [2.0, 2.0]);
    let source = doc
        .add_feature(
            &extrude(profile.0, &[&profile.1], 5.0, "join", &[local]),
            None,
        )
        .unwrap()
        .uid;
    doc.activate_component(mitcad_model::ids::ComponentUid::ROOT)
        .unwrap();
    let report = edit_preview(&mut doc, source, &profile, -5.0, "new_body", Some(true));
    assert_eq!(
        report["auto_operation"],
        json!({"operation":"cut","objects":[local]}),
        "root {root}: {report}"
    );
    doc.set_body_visible(local, false).unwrap();
    let report = edit_preview(&mut doc, source, &profile, -5.0, "new_body", Some(true));
    assert_eq!(
        report["auto_operation"],
        json!({"operation":"new_body","objects":[]}),
        "{report}"
    );
    doc.set_body_visible(local, true).unwrap();
    let report = edit_preview(&mut doc, source, &profile, -5.0, "new_body", Some(true));
    assert_eq!(
        report["auto_operation"]["objects"],
        json!([local]),
        "{report}"
    );
}

#[test]
fn edit_auto_report_preserves_actual_assembly_cut_scope_and_preview_geometry() {
    let mut doc = Document::new(MockKernel::default());
    let local = block(&mut doc);
    doc.create_component(Some("Other"), mitcad_model::Transform::IDENTITY, true)
        .unwrap();
    let foreign = block(&mut doc);
    doc.activate_component(mitcad_model::ids::ComponentUid::ROOT)
        .unwrap();
    let profile = square(&mut doc, [2.0, 2.0]);
    let scope = json!({"context":{"component":"C0","occurrence":""},
        "targets":{"type":"explicit","included":[{"component":"C1","occurrence":"O1"}]}});
    let mut definition =
        serde_json::to_value(extrude(profile.0, &[&profile.1], -5.0, "cut", &[])).unwrap();
    definition["cut_scope"] = scope.clone();
    let definition: mitcad_model::features::FeatureDef<mitcad_model::features::ValueInput> =
        serde_json::from_value(definition).unwrap();
    let mitcad_model::features::FeatureDef::Extrude(requested) = &definition else {
        panic!("expected an extrusion definition");
    };
    let expected_scope = requested.cut_scope.clone();
    let source = doc.add_feature(&definition, None).unwrap().uid;
    let original = unchanged_state(&doc);
    let expected = doc.preview_edit(source, &definition).unwrap();
    let expected_local = doc.preview_body(local).unwrap().clone();
    let expected_foreign = doc.preview_body(foreign).unwrap().clone();
    let (report, suggested) = doc.preview_edit_auto(source, &definition).unwrap();
    assert_eq!(report, expected);
    let suggested = suggested.unwrap();
    assert_eq!(suggested.operation, mitcad_model::features::Operation::Cut);
    assert_eq!(suggested.objects, [local]);
    assert_eq!(doc.preview_body(local), Some(&expected_local));
    assert_eq!(doc.preview_body(foreign), Some(&expected_foreign));
    let mitcad_model::features::FeatureDef::Extrude(previewed) =
        &doc.preview_feature().unwrap().def
    else {
        panic!("expected an extrusion preview");
    };
    assert_eq!(previewed.cut_scope, expected_scope);
    assert_eq!(unchanged_state(&doc), original);
}

#[test]
fn cancelled_edit_auto_preview_clears_temporary_results_and_keeps_the_document() {
    use mitcad_model::{ModelError, RecomputeMonitor};
    use std::sync::Arc;

    for during_contact in [false, true] {
        let mut doc = Document::new(MockKernel::default());
        let body = block(&mut doc);
        let profile = square(&mut doc, [2.0, 2.0]);
        let definition = extrude(profile.0, &[&profile.1], 5.0, "new_body", &[]);
        let source = doc.add_feature(&definition, None).unwrap().uid;
        doc.preview_edit(source, &definition).unwrap();
        assert!(doc.preview_feature().is_some());
        let original = unchanged_state(&doc);
        let shape = doc.body_shape(body).unwrap().clone();
        let monitor = Arc::new(RecomputeMonitor::new());
        if during_contact {
            *doc.kernel().cancel_on.borrow_mut() = Some(("tool_contacts", monitor.clone()));
        } else {
            monitor.cancel();
        }
        doc.set_monitor(Some(monitor));
        assert_eq!(
            doc.preview_edit_auto(source, &definition),
            Err(ModelError::Cancelled)
        );
        assert!(doc.preview_feature().is_none());
        assert!(doc.preview_tool().is_none());
        assert_eq!(unchanged_state(&doc), original);
        assert_eq!(doc.body_shape(body), Some(&shape));
        doc.set_monitor(None);
        *doc.kernel().cancel_on.borrow_mut() = None;
        assert!(
            doc.preview_edit_auto(source, &definition)
                .unwrap()
                .1
                .is_some()
        );
    }
}

#[test]
fn rejected_edit_auto_definition_does_not_leave_its_tool_only_preview() {
    let mut doc = Document::new(MockKernel::default());
    block(&mut doc);
    let profile = square(&mut doc, [2.0, 2.0]);
    let source = doc
        .add_feature(
            &extrude(profile.0, &[&profile.1], 5.0, "new_body", &[]),
            None,
        )
        .unwrap()
        .uid;
    let original = unchanged_state(&doc);
    let mut definition =
        serde_json::to_value(extrude(profile.0, &[&profile.1], -5.0, "cut", &[])).unwrap();
    definition["cut_scope"] = json!({"context":{"component":"C0","occurrence":""},
        "targets":{"type":"automatic","excluded":[]}, "initialize_on_cut":true});
    let definition = serde_json::from_value(definition).unwrap();
    let error = doc.preview_edit_auto(source, &definition).unwrap_err();
    assert!(error.to_string().contains("initialize_on_cut"), "{error}");
    assert!(doc.preview_feature().is_none());
    assert!(doc.preview_tool().is_none());
    assert_eq!(unchanged_state(&doc), original);
}
