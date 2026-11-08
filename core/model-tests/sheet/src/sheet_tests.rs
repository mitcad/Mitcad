// SPDX-License-Identifier: MIT
//! Sheet identities and atomic geometry/state lifetime, independent of OCCT.

use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, ResultStore, Transform};
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

fn rules() -> Value {
    json!({"type":"sheet_rules","thickness":1,"bend_radius":2,
        "allowance":{"type":"k_factor","k":0.4},"reference_side":"front",
        "bend_relief":{"type":"round","width":1,"depth":3},
        "corner_relief":{"type":"rectangular","width":1,"depth":3}})
}

fn add_sheet(doc: &mut Document<MockKernel>, rules: FeatureUid) -> (BodyUid, Value) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(40.0), &num(20.0))
        .unwrap()
        .region;
    let definition =
        json!({"type":"sheet_face","rules":rules,"profiles":[{"sketch":sketch,"region":region}]});
    let uid = doc.add_feature(&def(definition.clone()), None).unwrap().uid;
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
    (BodyUid::new(uid, 0), definition)
}

fn fixture() -> (Document<MockKernel>, FeatureUid, BodyUid, Value) {
    let mut doc = Document::new(MockKernel::default());
    let rules = doc.add_feature(&def(rules()), None).unwrap().uid;
    let (body, definition) = add_sheet(&mut doc, rules);
    (doc, rules, body, definition)
}

fn state(doc: &Document<MockKernel>, body: BodyUid) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query":"sheet_state","body":body}).to_string())
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn sheet_rules_are_parameterized_versioned_and_reference_side_moves_material() {
    let (mut doc, rules_id, body, _) = fixture();
    let before = state(&doc, body);
    assert_eq!(before["status"], "valid");
    assert_eq!(
        before["panels"][0]["region"]["loops"][0]["segments"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let mut changed = rules();
    changed["reference_side"] = json!("back");
    changed["thickness"] = json!(2);
    doc.edit_feature(rules_id, &def(changed)).unwrap();
    assert_eq!(
        doc.body_shape(body).unwrap().bounds.as_ref().unwrap().min[2],
        -2.0
    );
    let after = state(&doc, body);
    assert_ne!(before["rules_version"], after["rules_version"]);
    assert_ne!(before["shape_generation"], after["shape_generation"]);
    assert_eq!(before["panels"][0]["id"], after["panels"][0]["id"]);
    doc.undo();
    assert_eq!(state(&doc, body), before);
    doc.redo();
    assert_eq!(state(&doc, body), after);
    let query: Value = serde_json::from_str(
        &doc.query(&json!({"query":"sheet_rules","feature":rules_id}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(query["rules"]["thickness"], 2.0);
}

#[test]
fn sheet_missing_rules_failures_recover_after_suppression_undo_and_reopen() {
    let (mut doc, rules, body, _) = fixture();
    let valid = state(&doc, body);
    doc.set_suppressed(rules, true).unwrap();
    assert!(matches!(
        doc.status(body.feature),
        Some(FeatureStatus::Failed(_))
    ));
    assert_eq!(state(&doc, body)["status"], "unavailable");
    let mut opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    opened.recompute();
    assert_eq!(state(&opened, body)["status"], "unavailable");
    opened.set_suppressed(rules, false).unwrap();
    assert_eq!(state(&opened, body), valid);
    doc.undo();
    assert_eq!(state(&doc, body), valid);
    doc.set_marker(2).unwrap();
    assert_eq!(state(&doc, body)["status"], "unavailable");
    doc.set_marker(3).unwrap();
    assert_eq!(state(&doc, body), valid);
}

#[test]
fn sheet_invalid_rule_values_fail_at_recompute_and_are_unit_checked() {
    let (mut doc, rule, body, _) = fixture();
    let FeatureDef::SheetRules(rule_def) = &doc.feature(rule).unwrap().def else {
        panic!("rules")
    };
    let mitcad_model::features::sheet::Allowance::KFactor { k } = rule_def.allowance;
    let name = doc.parameters().name(k);
    assert!(doc.parameters().get(k).unwrap().unit().is_unitless());
    doc.set_parameter(&name, 1.2).unwrap();
    assert!(matches!(doc.status(rule), Some(FeatureStatus::Failed(_))));
    assert_eq!(state(&doc, body)["status"], "unavailable");
    doc.undo();
    assert_eq!(state(&doc, body)["status"], "valid");
    let mut bad = rules();
    bad["thickness"] = json!(0);
    assert!(doc.edit_feature(rule, &def(bad)).is_err());
}

#[test]
fn sheet_rigid_move_preserves_domains_and_generic_scale_invalidates_only_target() {
    let (mut doc, rules, body, _) = fixture();
    let (other, _) = add_sheet(&mut doc, rules);
    let moved = doc.add_feature(&def(json!({"type":"move","bodies":[body],"transform":{"type":"translate_xyz","x":4,"y":5,"z":6}})), None).unwrap().uid;
    assert_eq!(
        state(&doc, body)["panels"][0]["frame"]["origin"],
        json!([4.0, 5.0, 6.0])
    );
    assert_eq!(state(&doc, body)["status"], "valid");
    doc.add_feature(&def(json!({"type":"scale","bodies":[body],"point":{"type":"origin"},"scale":{"type":"uniform","factor":2}})), None).unwrap();
    assert_eq!(state(&doc, body)["status"], "invalid");
    assert_eq!(state(&doc, other)["status"], "valid");
    doc.undo();
    assert_eq!(doc.status(moved), Some(&FeatureStatus::Ok));
    assert_eq!(state(&doc, body)["status"], "valid");
}

#[test]
fn sheet_generic_failure_blocks_retained_shape_and_undo_clears_only_its_blocker() {
    let (mut doc, rules, body, _) = fixture();
    let (other, _) = add_sheet(&mut doc, rules);
    doc.kernel().fail.borrow_mut().insert("transform");
    let failed = doc.add_feature(&def(json!({"type":"move","bodies":[body],"transform":{"type":"translate_xyz","x":4,"y":0,"z":0}})), None).unwrap().uid;
    assert!(matches!(doc.status(failed), Some(FeatureStatus::Failed(_))));
    assert_eq!(state(&doc, body)["status"], "blocked");
    assert_eq!(state(&doc, body)["blockers"][0]["feature"], json!(failed));
    assert_eq!(state(&doc, other)["status"], "valid");
    doc.set_suppressed(failed, true).unwrap();
    assert_eq!(state(&doc, body)["status"], "valid");
    doc.set_suppressed(failed, false).unwrap();
    assert_eq!(state(&doc, body)["status"], "blocked");
    doc.undo();
    assert_eq!(state(&doc, body)["status"], "valid");
}

#[test]
fn sheet_body_copies_are_explicitly_invalid_and_do_not_invalidate_source() {
    let (mut doc, _, body, _) = fixture();
    let copy = doc.add_feature(&def(json!({"type":"move","bodies":[body],"copy":true,"transform":{"type":"translate_xyz","x":50,"y":0,"z":0}})), None).unwrap().uid;
    assert_eq!(state(&doc, body)["status"], "valid");
    assert_eq!(state(&doc, BodyUid::new(copy, 0))["status"], "invalid");
    doc.undo();
    doc.kernel().fail.borrow_mut().insert("transform");
    doc.add_feature(&def(json!({"type":"move","bodies":[body],"copy":true,"transform":{"type":"translate_xyz","x":60,"y":0,"z":0}})), None).unwrap();
    assert_eq!(state(&doc, body)["status"], "valid");
}

#[test]
fn sheet_component_copy_closes_external_rule_and_parameter_dependencies() {
    let mut doc = Document::new(MockKernel::default());
    doc.add_parameter("stock", 2.0, "").unwrap();
    let mut definition = rules();
    definition["thickness"] = json!("stock");
    let rules_id = doc.add_feature(&def(definition), None).unwrap().uid;
    let (component, occurrence) = doc
        .create_component(Some("Bracket"), Transform::IDENTITY, true)
        .unwrap();
    let (body, _) = add_sheet(&mut doc, rules_id);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (copy, _) = doc.paste_new(occurrence, None).unwrap();
    let copy_body = doc
        .bodies()
        .iter()
        .find(|b| doc.body_component(b.uid) == Some(copy))
        .unwrap()
        .uid;
    assert_ne!(copy, component);
    assert_eq!(state(&doc, copy_body)["status"], "valid");
    assert_ne!(state(&doc, copy_body)["rules"], json!(rules_id));
    doc.set_parameter("stock", 3.0).unwrap();
    assert_eq!(
        doc.body_shape(body).unwrap().bounds.as_ref().unwrap().max[2],
        3.0
    );
    assert_eq!(
        doc.body_shape(copy_body)
            .unwrap()
            .bounds
            .as_ref()
            .unwrap()
            .max[2],
        2.0
    );
    let mut opened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    opened.recompute();
    assert_eq!(state(&opened, copy_body), state(&doc, copy_body));
}

#[test]
fn sheet_component_from_bodies_changes_owner_atomically() {
    let (mut doc, _, body, _) = fixture();
    doc.add_feature(
        &def(json!({"type":"component_from_bodies","bodies":[body]})),
        None,
    )
    .unwrap();
    assert_eq!(state(&doc, body)["status"], "valid");
    assert_eq!(
        state(&doc, body)["owner"],
        json!(doc.body_component(body).unwrap())
    );
    assert_ne!(doc.body_component(body), Some(ComponentUid::ROOT));
    doc.undo();
    assert_eq!(state(&doc, body)["owner"], "C0");
}

#[test]
fn sheet_profile_slots_survive_reordering_removal_and_readdition() {
    let (mut doc, _, body, mut face) = fixture();
    let sketch: FeatureUid = serde_json::from_value(face["profiles"][0]["sketch"].clone()).unwrap();
    let added = doc
        .add_circle(sketch, [60.0, 0.0], &num(6.0))
        .unwrap()
        .region;
    face["profiles"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"sketch":sketch,"region":added}));
    doc.edit_feature(body.feature, &def(face.clone())).unwrap();
    let before = state(&doc, body);
    let island = BodyUid::new(body.feature, 1);
    assert_eq!(state(&doc, island)["status"], "valid");
    face["profiles"].as_array_mut().unwrap().reverse();
    doc.edit_feature(body.feature, &def(face.clone())).unwrap();
    assert_eq!(state(&doc, body)["panels"], before["panels"]);
    let mut removed = face.clone();
    removed["profiles"].as_array_mut().unwrap().pop();
    doc.edit_feature(body.feature, &def(removed)).unwrap();
    assert_eq!(state(&doc, island)["status"], "unavailable");
    doc.edit_feature(body.feature, &def(face)).unwrap();
    assert_eq!(state(&doc, island)["panels"][0]["id"]["slot"], 1);
}

#[test]
fn sheet_results_store_and_fork_restore_shape_and_state_as_one_result() {
    let (mut doc, _, body, _) = fixture();
    let dir = mitcad_model::testing::unique_temp_path("sheet-store");
    let store = ResultStore::new(&dir, "sheet-test");
    doc.set_result_store(Some(store.clone()));
    let saved = doc.to_json();
    let expected = state(&doc, body);
    assert!(doc.persist_results(std::time::Duration::ZERO).results >= 1);
    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    opened.set_result_store(Some(store));
    opened.recompute();
    assert!(opened.last_result().restored.contains(&body.feature));
    assert_eq!(state(&opened, body), expected);
    assert_eq!(opened.body_shape(body), doc.body_shape(body));
    assert_eq!(state(&doc.fork().unwrap(), body), expected);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn sheet_face_keeps_exact_hole_curves_and_profile_provenance() {
    let mut doc = Document::new(MockKernel::default());
    let rules = doc.add_feature(&def(rules()), None).unwrap().uid;
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(40.0), &num(20.0))
        .unwrap()
        .region;
    doc.add_circle(sketch, [10.0, 10.0], &num(4.0)).unwrap();
    let face = doc.add_feature(&def(json!({"type":"sheet_face","rules":rules,"profiles":[{"sketch":sketch,"region":region}]})), None).unwrap().uid;
    let state = state(&doc, BodyUid::new(face, 0));
    assert_eq!(state["status"], "valid");
    let loops = state["panels"][0]["region"]["loops"].as_array().unwrap();
    assert_eq!(loops.len(), 2);
    assert_eq!(loops[1]["segments"][0]["geometry"]["type"], "circle");
    assert_eq!(loops[1]["segments"][0]["geometry"]["radius"], 2.0);
    assert_eq!(loops[1]["segments"][0]["id"]["sketch"], json!(sketch));
}

#[test]
fn sheet_scoped_cut_invalidates_foreign_split_owners_and_failure_excludes_b() {
    let mut doc = Document::new(MockKernel::default());
    let rules = doc.add_feature(&def(rules()), None).unwrap().uid;
    let mut bodies = Vec::new();
    for (i, name) in ["A", "B", "C"].iter().enumerate() {
        doc.activate_component(ComponentUid::ROOT).unwrap();
        doc.create_component(
            Some(name),
            Transform::translation([0.0, 0.0, i as f64 * 2.0]),
            true,
        )
        .unwrap();
        bodies.push(add_sheet(&mut doc, rules).0);
    }
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [5.0, 5.0], &num(2.0), &num(2.0))
        .unwrap()
        .region;
    let cut = json!({"type":"extrude","profiles":[{"sketch":sketch,"region":region}],"extent":{"type":"distance","distance":8},"operation":"cut",
        "cut_scope":{"context":{"component":"C0","occurrence":""},"targets":{"type":"automatic","excluded":[{"component":"C2","occurrence":"O2"}]}}});
    doc.kernel().split.set(Some(2));
    let uid = doc.add_feature(&def(cut.clone()), None).unwrap().uid;
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
    for index in [0, 2] {
        assert_eq!(state(&doc, bodies[index])["status"], "invalid");
        assert_eq!(
            doc.body_component(bodies[index]),
            Some(ComponentUid(index as u32 + 1))
        );
    }
    assert_eq!(state(&doc, bodies[1])["status"], "valid");
    for body in doc.bodies().iter().filter(|b| b.uid.feature == uid) {
        assert_eq!(state(&doc, body.uid)["status"], "invalid");
        assert_ne!(doc.body_component(body.uid), Some(ComponentUid::ROOT));
    }
    doc.undo();
    doc.kernel().fail.borrow_mut().insert("cut");
    let mut failing = cut;
    failing["extent"]["distance"] = json!(9);
    let failed = doc.add_feature(&def(failing), None).unwrap().uid;
    assert!(matches!(doc.status(failed), Some(FeatureStatus::Failed(_))));
    assert_eq!(state(&doc, bodies[0])["status"], "blocked");
    assert_eq!(state(&doc, bodies[1])["status"], "valid");
    assert_eq!(state(&doc, bodies[2])["status"], "blocked");
    doc.set_suppressed(failed, true).unwrap();
    assert!(bodies.iter().all(|b| state(&doc, *b)["status"] == "valid"));
}

#[test]
fn sheet_preview_is_atomic_and_cancel_does_not_publish_rules_or_bodies() {
    let (mut doc, rules, body, _) = fixture();
    let before = state(&doc, body);
    let mut next = self::rules();
    next["thickness"] = json!(3);
    let preview = doc.preview_edit(rules, &def(next)).unwrap();
    assert_eq!(preview.status, FeatureStatus::Ok);
    assert_ne!(
        preview.sheet_states[0]["shape_generation"],
        before["shape_generation"]
    );
    assert_eq!(state(&doc, body), before);
    doc.clear_preview();
    assert_eq!(state(&doc, body), before);
}

#[test]
fn sheet_failed_store_entry_restores_footprint_without_blocking_unrelated_body() {
    let (mut doc, rules, body, _) = fixture();
    let (other, _) = add_sheet(&mut doc, rules);
    doc.kernel().fail.borrow_mut().insert("transform");
    let failed = doc.add_feature(&def(json!({"type":"move","bodies":[body],"transform":{"type":"translate_xyz","x":7,"y":0,"z":0}})), None).unwrap().uid;
    let saved = doc.to_json();
    let expected = state(&doc, body);
    assert_eq!(expected["status"], "blocked");
    let mut normal = Document::from_json(&saved, MockKernel::default()).unwrap();
    normal.kernel().fail.borrow_mut().insert("transform");
    normal.recompute();
    assert_eq!(state(&normal, body), expected);
    assert_eq!(state(&normal, other)["status"], "valid");
    let dir = mitcad_model::testing::unique_temp_path("sheet-failed-store");
    let store = ResultStore::new(&dir, "sheet-failed-test");
    doc.set_result_store(Some(store.clone()));
    doc.persist_results(std::time::Duration::ZERO);
    // The normal persistence policy skips failures. Exercise the complete
    // failed-entry protocol directly, without changing that policy.
    store
        .write(
            doc.kernel(),
            doc.feature(failed).unwrap(),
            doc.last_result()
                .result(failed)
                .unwrap()
                .output
                .as_ref()
                .unwrap(),
            doc.parameters(),
        )
        .unwrap();
    let mut restored = Document::from_json(&saved, MockKernel::default()).unwrap();
    restored.set_result_store(Some(store));
    restored.recompute();
    assert!(restored.last_result().restored.contains(&failed));
    assert_eq!(state(&restored, body), expected);
    assert_eq!(state(&restored, other)["status"], "valid");
    restored.set_suppressed(failed, true).unwrap();
    assert_eq!(state(&restored, body)["status"], "valid");
    restored.undo();
    assert_eq!(state(&restored, body), expected);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn sheet_failed_combined_mirror_blocks_original_but_copy_only_mirror_does_not() {
    let (mut doc, rules, body, _) = fixture();
    let (other, _) = add_sheet(&mut doc, rules);
    doc.kernel().fail.borrow_mut().insert("transform");
    let mut mirror = json!({"type":"mirror","objects":{"type":"bodies","bodies":[body]},"plane":"yz","combine":true});
    let failed = doc.add_feature(&def(mirror.clone()), None).unwrap().uid;
    assert!(matches!(doc.status(failed), Some(FeatureStatus::Failed(_))));
    assert_eq!(state(&doc, body)["status"], "blocked");
    assert_eq!(state(&doc, body)["blockers"][0]["feature"], json!(failed));
    assert_eq!(state(&doc, other)["status"], "valid");
    mirror["combine"] = json!(false);
    doc.edit_feature(failed, &def(mirror)).unwrap();
    assert!(matches!(doc.status(failed), Some(FeatureStatus::Failed(_))));
    assert_eq!(state(&doc, body)["status"], "valid");
    doc.undo();
    assert_eq!(state(&doc, body)["status"], "blocked");
}

fn flange_definition(body: BodyUid, sketch: FeatureUid) -> Value {
    json!({"type":"sheet_flange","body":body,
        "boundary":{"panel":{"feature":body.feature,"slot":0},"curve":{"type":"sketch","sketch":sketch,"segment":"c2[c1,c3]"}},
        "angle":std::f64::consts::FRAC_PI_2,"length":{"mode":"tangent","value":15},"position":"inside"})
}

#[test]
fn folded_failure_blocks_only_its_input_and_survives_reopen_until_suppressed() {
    let (mut doc, rules, body, face) = fixture();
    let (other, _) = add_sheet(&mut doc, rules);
    let sketch: FeatureUid = serde_json::from_value(face["profiles"][0]["sketch"].clone()).unwrap();
    let original = doc.body_shape(body).cloned();
    let mut flange = flange_definition(body, sketch);
    flange["angle"] = json!(0);
    let failed = doc.add_feature(&def(flange.clone()), None).unwrap().uid;
    assert!(
        matches!(doc.status(failed),Some(FeatureStatus::Failed(message)) if message.contains("angle"))
    );
    assert_eq!(doc.body_shape(body), original.as_ref());
    assert_eq!(state(&doc, body)["status"], "blocked");
    assert_eq!(state(&doc, other)["status"], "valid");
    let saved = doc.to_json();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(state(&reopened, body), state(&doc, body));
    reopened.set_suppressed(failed, true).unwrap();
    assert_eq!(state(&reopened, body)["status"], "valid");
    reopened.undo();
    assert_eq!(state(&reopened, body)["status"], "blocked");
    // A later feature cannot turn retained upstream failure geometry back
    // into valid sheet material, even when its own numeric values are valid.
    let later = reopened
        .add_feature(&def(flange_definition(body, sketch)), None)
        .unwrap()
        .uid;
    assert!(
        matches!(reopened.status(later),Some(FeatureStatus::Failed(message)) if message.contains("blocked"))
    );
    assert_eq!(
        state(&reopened, body)["blockers"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn folded_endpoint_clearance_and_depth_are_explicit_parameters() {
    let (mut doc, _, body, face) = fixture();
    let sketch: FeatureUid = serde_json::from_value(face["profiles"][0]["sketch"].clone()).unwrap();
    let mut flange = flange_definition(body, sketch);
    flange["offsets"] = json!({"start":3,"end":4});
    flange["relief"] = json!({"type":"tear","width":0,"depth":3});
    let failed = doc.add_feature(&def(flange.clone()), None).unwrap().uid;
    assert!(
        matches!(doc.status(failed),Some(FeatureStatus::Failed(message)) if message.contains("positive"))
    );
    flange["relief"]["width"] = json!(-1);
    doc.edit_feature(failed, &def(flange.clone())).unwrap();
    assert!(
        matches!(doc.status(failed),Some(FeatureStatus::Failed(message)) if message.contains("positive"))
    );
    // Any positive depth separates the material (mitcad#242: the imports'
    // rules use half a thickness); none is refused.
    flange["relief"]["width"] = json!(2);
    flange["relief"]["depth"] = json!(0);
    doc.edit_feature(failed, &def(flange.clone())).unwrap();
    assert!(
        matches!(doc.status(failed),Some(FeatureStatus::Failed(message)) if message.contains("positive"))
    );
    assert_eq!(state(&doc, body)["status"], "blocked");
    doc.set_suppressed(failed, true).unwrap();
    assert_eq!(state(&doc, body)["status"], "valid");
    doc.undo();
    assert_eq!(state(&doc, body)["status"], "blocked");
    let FeatureDef::SheetFlange(d) = &doc.feature(failed).unwrap().def else {
        panic!("flange definition");
    };
    let mut slots = Vec::new();
    d.map_params(&mut |slot, p| {
        slots.push((slot.to_owned(), *p));
        Ok::<_, ()>(*p)
    })
    .unwrap();
    assert!(slots.iter().any(|(slot, _)| slot == "relief.width"));
    assert!(slots.iter().any(|(slot, _)| slot == "relief.depth"));
    assert!(slots.iter().any(|(slot, _)| slot == "offsets.start"));
}

#[test]
fn interior_fold_checks_relief_before_any_geometry_and_tracks_line_dependencies() {
    let (mut doc, _, body, _) = fixture();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    doc.command(
        &json!({"cmd":"sketch.add_line","sketch":sketch,"start":[20,3],"end":[20,16]}).to_string(),
    )
    .unwrap();
    let fold = json!({"type":"sheet_fold","body":body,"panel":{"feature":body.feature,"slot":0},"line":{"sketch":sketch,"curve":"c1"},
        "stationary_side":"left","angle":std::f64::consts::FRAC_PI_2,"relief":{"type":"round","width":2,"depth":0}});
    let failed = doc.add_feature(&def(fold), None).unwrap().uid;
    assert!(
        matches!(doc.status(failed),Some(FeatureStatus::Failed(message)) if message.contains("behind the tangent"))
    );
    assert_eq!(doc.kernel().calls.borrow().get("sheet_trim_domain"), None);
    assert!(
        doc.feature(failed)
            .unwrap()
            .def
            .info()
            .references()
            .features
            .contains(&sketch)
    );
    assert_eq!(state(&doc, body)["status"], "blocked");
    doc.set_suppressed(failed, true).unwrap();
    assert_eq!(state(&doc, body)["status"], "valid");
    doc.undo();
    assert_eq!(state(&doc, body)["status"], "blocked");
}

#[test]
fn positioned_bend_failure_keeps_both_bodies_and_blocks_both_inputs() {
    let (mut doc, rules, first, face) = fixture();
    let (second, second_face) = add_sheet(&mut doc, rules);
    let boundary = |body: BodyUid, face: &Value, segment: &str| json!({"panel":{"feature":body.feature,"slot":0},"curve":{"type":"sketch","sketch":face["profiles"][0]["sketch"],"segment":segment}});
    let first_shape = doc.body_shape(first).cloned();
    let second_shape = doc.body_shape(second).cloned();
    let failed=doc.add_feature(&def(json!({"type":"sheet_bend","body":first,"moving_body":second,"angle":std::f64::consts::FRAC_PI_2,
        "stationary":boundary(first,&face,"c2[c1,c3]"),"moving":boundary(second,&second_face,"c4[c3,c1]")})),None).unwrap().uid;
    assert!(
        matches!(doc.status(failed),Some(FeatureStatus::Failed(message)) if message.contains("tangent ports"))
    );
    assert_eq!(doc.body_shape(first), first_shape.as_ref());
    assert_eq!(doc.body_shape(second), second_shape.as_ref());
    assert_eq!(state(&doc, first)["status"], "blocked");
    assert_eq!(state(&doc, second)["status"], "blocked");
    let FeatureDef::SheetBend(d) = &doc.feature(failed).unwrap().def else {
        panic!("bend definition");
    };
    assert_eq!(
        d.port_mode,
        Some(mitcad_model::sheet::BendPortMode::TangentPorts)
    );
    doc.undo();
    assert_eq!(state(&doc, first)["status"], "valid");
    assert_eq!(state(&doc, second)["status"], "valid");
    doc.redo();
    assert_eq!(state(&doc, second)["status"], "blocked");
}

#[test]
fn ordered_corner_slots_keep_tombstones_through_reorder_remove_and_reopen() {
    let (mut doc, _, body, face) = fixture();
    let sketch = face["profiles"][0]["sketch"].clone();
    let corner = |incoming: &str, outgoing: &str| json!({"panel":{"feature":body.feature,"slot":0},"incoming":{"type":"sketch","sketch":sketch,"segment":incoming},"outgoing":{"type":"sketch","sketch":sketch,"segment":outgoing}});
    let a = corner("c1[c4,c2]", "c2[c1,c3]");
    let b = corner("c2[c1,c3]", "c3[c2,c4]");
    let mut definition = json!({"type":"sheet_corner_chamfer","body":body,"corners":[a,b],"distances":{"first":2,"second":3}});
    let uid = doc.add_feature(&def(definition.clone()), None).unwrap().uid;
    let slots = |doc: &Document<MockKernel>| match &doc.feature(uid).unwrap().def {
        FeatureDef::SheetCornerChamfer(d) => d.corner_slots.clone(),
        _ => panic!("corner definition"),
    };
    let original = slots(&doc);
    assert_eq!(original.len(), 2);
    definition["corners"] = json!([b, a]);
    doc.edit_feature(uid, &def(definition.clone())).unwrap();
    assert_eq!(slots(&doc), original);
    definition["corners"] = json!([a]);
    doc.edit_feature(uid, &def(definition.clone())).unwrap();
    assert_eq!(slots(&doc), original);
    definition["corners"] = json!([a, b]);
    doc.edit_feature(uid, &def(definition)).unwrap();
    assert_eq!(slots(&doc), original);
    let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(slots(&reopened), original);
}

/// The fixture face's sketch (Sketch1: the 40 x 20 rectangle, `c1` bottom,
/// `c2` right, `c3` top, `c4` left) and its stored profile key.
fn face_profile(face: &Value) -> (FeatureUid, String) {
    (
        serde_json::from_value(face["profiles"][0]["sketch"].clone()).unwrap(),
        face["profiles"][0]["region"].as_str().unwrap().to_owned(),
    )
}

fn failed_with(doc: &Document<MockKernel>, uid: FeatureUid, part: &str) -> bool {
    matches!(doc.status(uid), Some(FeatureStatus::Failed(message)) if message.contains(part))
}

#[test]
fn sheet_face_follows_a_changed_outline_and_keeps_its_panel() {
    // mitcad#1444: rounding a corner gives the region a new key; the Face
    // finds it as Extrude does, warns, and keeps the panel's slot.
    let (mut doc, _, body, face) = fixture();
    let (sketch, key) = face_profile(&face);
    let before = state(&doc, body);
    doc.command(
        &json!({"cmd":"sketch.fillet","sketch":sketch,"a":"c3","b":"c4","radius":5}).to_string(),
    )
    .unwrap();
    assert_eq!(doc.status(body.feature), Some(&FeatureStatus::Ok));
    let warnings = doc.warnings(body.feature);
    let warning = warnings
        .iter()
        .find(|w| w.starts_with(&format!("region {key} found as r{{")))
        .unwrap_or_else(|| panic!("no warning about {key}: {warnings:?}"));
    assert!(warning.ends_with(" after a change in Sketch1"), "{warning}");
    let after = state(&doc, body);
    assert_eq!(after["status"], "valid");
    assert_eq!(after["panels"].as_array().unwrap().len(), 1);
    assert_eq!(after["panels"][0]["id"], before["panels"][0]["id"]);
    // The rounded outline: four lines and the arc; the walls along the
    // unchanged bottom and right sides keep their curve segments' names.
    let segments: Vec<Value> = after["panels"][0]["region"]["loops"][0]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"]["segment"].clone())
        .collect();
    assert_eq!(segments.len(), 5, "{segments:?}");
    assert!(segments.contains(&json!("c1[c4,c2]")), "{segments:?}");
    assert!(segments.contains(&json!("c2[c1,c3]")), "{segments:?}");
    // The definition keeps its reference and its slot.
    let FeatureDef::SheetFace(d) = &doc.feature(body.feature).unwrap().def else {
        panic!("face definition");
    };
    assert_eq!(d.profiles[0].region.to_string(), key);
    assert_eq!(d.panel_slots.len(), 1);
    assert_eq!(d.panel_slots[0].slot, 0);
    // Undone, the stored key is the region's own again: no warning.
    doc.undo();
    assert_eq!(doc.status(body.feature), Some(&FeatureStatus::Ok));
    assert!(doc.warnings(body.feature).is_empty());
    assert_eq!(state(&doc, body)["panels"], before["panels"]);
}

#[test]
fn sheet_face_refuses_a_split_profile_until_its_pieces_are_selected() {
    // mitcad#1444: a line across the rectangle splits the region in two.
    let (mut doc, _, body, mut face) = fixture();
    let (sketch, key) = face_profile(&face);
    doc.command(
        &json!({"cmd":"sketch.add_line","sketch":sketch,"start":[10,0],"end":[10,20]}).to_string(),
    )
    .unwrap();
    assert!(
        failed_with(
            &doc,
            body.feature,
            &format!("region {key} is split into r{{")
        ),
        "{:?}",
        doc.status(body.feature)
    );
    assert!(failed_with(
        &doc,
        body.feature,
        " after a change in Sketch1; select the profiles again"
    ));
    // The stored reference still selects the main piece; selecting the
    // other piece too gives it a slot of its own and the main piece keeps
    // slot 0.
    let (main, other) = {
        let output = doc.sketch_output(sketch).unwrap();
        let found = output.found(&key.parse().unwrap()).unwrap();
        assert_eq!(found.regions.len(), 2);
        (
            found.regions[0].profile.key.to_string(),
            found.regions[1].profile.key.to_string(),
        )
    };
    face["profiles"]
        .as_array_mut()
        .unwrap()
        .push(json!({"sketch":sketch,"region":other}));
    doc.edit_feature(body.feature, &def(face)).unwrap();
    assert_eq!(doc.status(body.feature), Some(&FeatureStatus::Ok));
    assert!(
        doc.warnings(body.feature)
            .iter()
            .any(|w| w.contains(&format!("region {key} found as {main} + {other}"))),
        "{:?}",
        doc.warnings(body.feature)
    );
    let FeatureDef::SheetFace(d) = &doc.feature(body.feature).unwrap().def else {
        panic!("face definition");
    };
    let slot_of = |region: &str| {
        d.panel_slots
            .iter()
            .find(|s| s.profile.region.to_string() == region)
            .map(|s| s.slot)
    };
    assert_eq!(slot_of(&key), Some(0));
    assert_eq!(slot_of(&other), Some(1));
    // The mock kernel makes a solid of each panel: slot 0 the main piece.
    for slot in [0, 1] {
        let piece = state(&doc, BodyUid::new(body.feature, slot));
        assert_eq!(piece["status"], "valid");
        assert_eq!(piece["panels"][0]["id"]["slot"], slot);
    }
}

#[test]
fn sheet_face_whose_profile_is_gone_names_its_sketch() {
    // mitcad#1444: a circle beside the rectangle, then the rectangle's
    // curves deleted: no region shares a curve with the stored key. (The
    // circle is drawn first: drawn into the emptied sketch it would take
    // the id c1, as ids continue from the highest one in use, and the key
    // would find its region by that curve, as an extrusion's does.)
    let (mut doc, _, body, face) = fixture();
    let (sketch, key) = face_profile(&face);
    doc.add_circle(sketch, [60.0, 0.0], &num(6.0)).unwrap();
    doc.command(
        &json!({"cmd":"sketch.remove","sketch":sketch,"items":["c1","c2","c3","c4"]}).to_string(),
    )
    .unwrap();
    assert!(
        failed_with(&doc, body.feature, &format!("Sketch1 has no profile {key}")),
        "{:?}",
        doc.status(body.feature)
    );
    assert_eq!(state(&doc, body)["status"], "unavailable");
}
