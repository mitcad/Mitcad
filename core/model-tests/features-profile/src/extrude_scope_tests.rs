// SPDX-License-Identifier: MIT
//! Assembly cut scope: persistent selection, ownership, cache inputs and errors.

use serde_json::{Value, json};

use mitcad_model::features::extrude_scope::CutScope;
use mitcad_model::features::{FeatureDef, SketchPlane, ValueInput};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, ModelError, Transform};
use mitcad_model_api::DocumentApi;

struct Fixture {
    doc: Document<MockKernel>,
    bodies: [BodyUid; 3],
    cut: Value,
}

fn box_body(doc: &mut Document<MockKernel>) -> BodyUid {
    let uid = doc
        .add_feature(
            &def(json!({"type":"box", "plane":"xy", "corner":[0,0],
        "length":10, "width":10, "height":10, "operation":"new_body"})),
            None,
        )
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

fn fixture() -> Fixture {
    let mut doc = Document::new(MockKernel::default());
    let mut bodies = Vec::new();
    for (i, name) in ["A", "B", "C"].into_iter().enumerate() {
        doc.activate_component(ComponentUid::ROOT).unwrap();
        doc.create_component(
            Some(name),
            Transform::translation([0.0, 0.0, i as f64 * 20.0]),
            true,
        )
        .unwrap();
        bodies.push(box_body(&mut doc));
    }
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    Fixture {
        doc,
        bodies: bodies.try_into().unwrap(),
        cut: json!({"type":"extrude",
        "profiles":[{"sketch":sketch,"region":region}], "extent":{"type":"distance","distance":60},
        "operation":"cut", "cut_scope":{"context":{"component":"C0","occurrence":""},
            "targets":{"type":"automatic","excluded":[{"component":"C2","occurrence":"O2"}]}}}),
    }
}

fn is_cut(doc: &Document<MockKernel>, body: BodyUid) -> bool {
    doc.body_shape(body).unwrap().history.starts_with("cut(")
}

fn cut_definition(value: &Value) -> FeatureDef<ValueInput> {
    def(value.clone())
}

#[test]
fn cut_scope_excludes_b_preserves_owners_and_metadata_through_undo_reopen() {
    let mut f = fixture();
    f.doc.rename_body(f.bodies[0], "A solid").unwrap();
    let report = f.doc.preview_add(&cut_definition(&f.cut), None).unwrap();
    assert_eq!(report.status, FeatureStatus::Ok);
    let targets = report.cut_targets.unwrap();
    assert!(targets.can_commit);
    assert_eq!(
        (targets.selected_components, targets.selected_bodies),
        (2, 2)
    );
    assert_eq!(
        targets
            .rows
            .iter()
            .map(|r| (r.name.as_str(), r.checked))
            .collect::<Vec<_>>(),
        [("A", true), ("B", false), ("C", true)]
    );
    assert_eq!(targets.rows[0].bodies[0].name, "A solid");
    assert!(
        f.bodies.iter().all(|b| !is_cut(&f.doc, *b)),
        "preview cannot mutate committed bodies"
    );
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    for (i, b) in f.bodies.iter().enumerate() {
        assert_eq!(is_cut(&f.doc, *b), i != 1);
        assert_eq!(f.doc.body_component(*b), Some(ComponentUid(i as u32 + 1)));
    }
    f.doc.undo();
    assert!(f.bodies.iter().all(|b| !is_cut(&f.doc, *b)));
    f.doc.redo();
    let saved = f.doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(loaded.status(uid), Some(&FeatureStatus::Ok));
    assert_eq!(loaded.to_json(), saved);
    for (i, b) in f.bodies.iter().enumerate() {
        assert_eq!(is_cut(&loaded, *b), i != 1);
        assert_eq!(loaded.body_component(*b), Some(ComponentUid(i as u32 + 1)));
    }
}

#[test]
fn cut_scope_empty_explicit_selection_is_not_legacy_or_automatic() {
    let mut f = fixture();
    f.cut["cut_scope"]["targets"] = json!({"type":"explicit","included":[]});
    let report = f.doc.preview_add(&cut_definition(&f.cut), None).unwrap();
    assert!(
        matches!(report.status, FeatureStatus::Failed(ref message) if message.contains("No components selected"))
    );
    let targets = report.cut_targets.unwrap();
    assert!(!targets.can_commit);
    assert_eq!(targets.rows.len(), 3);
    assert_eq!(targets.selected_bodies, 0);
    assert!(targets.rows.iter().all(|r| !r.checked));
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    assert!(matches!(f.doc.status(uid), Some(FeatureStatus::Failed(_))));
    assert!(f.bodies.iter().all(|b| !is_cut(&f.doc, *b)));
    f.doc.undo();
    f.cut.as_object_mut().unwrap().remove("cut_scope");
    let report = f.doc.preview_add(&cut_definition(&f.cut), None).unwrap();
    assert!(
        report.cut_targets.is_none(),
        "legacy local behavior has no assembly scope"
    );
    assert!(
        matches!(report.status, FeatureStatus::Failed(_)),
        "the root has no local solid to cut"
    );
}

#[test]
fn cut_scope_multibody_exclusion_and_split_keep_target_ownership() {
    let mut f = fixture();
    f.doc.activate_component(ComponentUid(1)).unwrap();
    let untouched = box_body(&mut f.doc);
    f.doc.activate_component(ComponentUid::ROOT).unwrap();
    f.cut["cut_scope"]["targets"]["excluded_bodies"] = json!([
        {"component":"C1", "occurrence":"O1", "body":untouched}]);
    f.doc.kernel().split.set(Some(2));
    let preview = f.doc.preview_add(&cut_definition(&f.cut), None).unwrap();
    let targets = preview.cut_targets.unwrap();
    assert_eq!(targets.rows[0].bodies.len(), 2);
    assert!(targets.rows[0].partial);
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    assert_eq!(
        f.doc.body_component(BodyUid::new(uid, 0)),
        Some(ComponentUid(1))
    );
    assert_eq!(
        f.doc.body_component(BodyUid::new(uid, 1)),
        Some(ComponentUid(3))
    );
    assert_eq!(f.doc.component_bodies(ComponentUid(1)).len(), 3);
    assert_eq!(f.doc.component_bodies(ComponentUid(3)).len(), 2);
    assert!(!is_cut(&f.doc, untouched));
    assert_eq!(f.doc.component_bodies(ComponentUid::ROOT).len(), 0);
}

#[test]
fn cut_scope_hidden_second_occurrence_invalidates_cache_and_disables_shared_definition() {
    let mut f = fixture();
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    assert!(is_cut(&f.doc, f.bodies[0]));
    // It does not intersect the tool, but editing the definition would affect it.
    let second = f
        .doc
        .copy_occurrence(
            OccurrenceUid(1),
            Some(Transform::translation([100.0, 0.0, 0.0])),
        )
        .unwrap();
    f.doc.set_occurrence_visible(second, false).unwrap();
    assert!(!is_cut(&f.doc, f.bodies[0]));
    assert!(is_cut(&f.doc, f.bodies[2]));
    let report = f.doc.preview_edit(uid, &cut_definition(&f.cut)).unwrap();
    let targets = report.cut_targets.unwrap();
    let a = targets
        .rows
        .iter()
        .find(|r| r.component == ComponentUid(1))
        .unwrap();
    assert!(!a.eligible && !a.checked);
    assert!(
        a.reason
            .as_deref()
            .unwrap()
            .contains("multiple occurrences")
    );
    f.cut["cut_scope"]["targets"] =
        json!({"type":"explicit","included":[{"component":"C1","occurrence":"O1"}]});
    let report = f.doc.preview_edit(uid, &cut_definition(&f.cut)).unwrap();
    assert!(matches!(report.status, FeatureStatus::Failed(_)));
    assert!(!report.cut_targets.unwrap().can_commit);
    f.doc.undo(); // visibility
    f.doc.undo(); // second occurrence
    assert!(is_cut(&f.doc, f.bodies[0]));
}

#[test]
fn cut_scope_missing_exclusion_fails_and_repaired_definition_recovers() {
    let mut f = fixture();
    let correct = f.cut.clone();
    f.cut["cut_scope"]["targets"]["excluded"][0]["occurrence"] = json!("O999");
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    assert!(matches!(f.doc.status(uid), Some(FeatureStatus::Failed(_))));
    assert!(f.bodies.iter().all(|b| !is_cut(&f.doc, *b)));
    let saved = f.doc.to_json();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.recompute();
    assert!(matches!(
        reopened.status(uid),
        Some(FeatureStatus::Failed(_))
    ));
    reopened
        .edit_feature(uid, &cut_definition(&correct))
        .unwrap();
    assert_eq!(reopened.status(uid), Some(&FeatureStatus::Ok));
    assert!(!is_cut(&reopened, f.bodies[1]));
    reopened.undo();
    assert!(matches!(
        reopened.status(uid),
        Some(FeatureStatus::Failed(_))
    ));
    reopened.redo();
    assert_eq!(reopened.status(uid), Some(&FeatureStatus::Ok));
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn scope_of(doc: &Document<MockKernel>, uid: FeatureUid) -> CutScope {
    let FeatureDef::Extrude(extrude) = &doc.feature(uid).unwrap().def else {
        panic!("expected an extrusion");
    };
    extrude.cut_scope.clone().unwrap()
}

#[test]
fn cut_scope_body_and_component_references_are_dependencies() {
    let mut f = fixture();
    f.cut["cut_scope"]["targets"] = json!({"type":"explicit",
        "included":[{"component":"C1","occurrence":"O1"}]});
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    // An included component is a real reference: the `dependents` query
    // of its feature lists the cut (mitcad#1236 keeps it).
    let dependents = query(
        &f.doc,
        json!({"query":"dependents","uid":f.bodies[0].feature}),
    );
    assert_eq!(dependents["dependents"][0]["uid"], json!(uid));
    assert!(matches!(
        f.doc.delete_feature(f.bodies[0].feature, false),
        Err(ModelError::Dependents { .. })
    ));
    f.doc.set_suppressed(f.bodies[0].feature, true).unwrap();
    assert!(matches!(f.doc.status(uid), Some(FeatureStatus::Failed(_))));
    f.doc.undo();
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    f.cut["cut_scope"]["targets"]["excluded_bodies"] = json!([
        {"component":"C2","occurrence":"O2","body":f.bodies[1]}]);
    f.doc.edit_feature(uid, &cut_definition(&f.cut)).unwrap();
    assert!(f.doc.move_feature(uid, 0).is_err());
    // An excluded body is no reference (mitcad#1236): its maker may move
    // after the cut, which then has nothing there to leave alone.
    let last = f.doc.features().count() - 1;
    f.doc.move_feature(f.bodies[1].feature, last).unwrap();
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    assert!(is_cut(&f.doc, f.bodies[0]));
    assert!(!is_cut(&f.doc, f.bodies[1]));
}

/// mitcad#1236: Part (C1 at O1) with two boxes; Holder (C2 at O2, at the
/// same place) with a box and a cut of its own whose automatic scope
/// excludes Part:1.
struct Excluding {
    doc: Document<MockKernel>,
    part: [BodyUid; 2],
    holder: BodyUid,
    cut: FeatureUid,
    definition: Value,
}

fn excluding() -> Excluding {
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("Part"), Transform::IDENTITY, true)
        .unwrap();
    let part = [box_body(&mut doc), box_body(&mut doc)];
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.create_component(Some("Holder"), Transform::IDENTITY, true)
        .unwrap();
    let holder = box_body(&mut doc);
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let definition = json!({"type":"extrude",
        "profiles":[{"sketch":sketch,"region":region}], "extent":{"type":"distance","distance":60},
        "operation":"cut", "cut_scope":{"context":{"component":"C2","occurrence":"O2"},
            "targets":{"type":"automatic","excluded":[{"component":"C1","occurrence":"O1"}]}}});
    let cut = doc
        .add_feature(&cut_definition(&definition), None)
        .unwrap()
        .uid;
    doc.activate_component(ComponentUid::ROOT).unwrap();
    assert_eq!(doc.status(cut), Some(&FeatureStatus::Ok));
    assert!(is_cut(&doc, holder));
    assert!(part.iter().all(|b| !is_cut(&doc, *b)));
    Excluding {
        doc,
        part,
        holder,
        cut,
        definition,
    }
}

#[test]
fn cut_scope_exclusion_makes_no_dependent_of_the_excluded_component() {
    let mut f = excluding();
    let last = f.part[1].feature;
    for feature in [f.part[0].feature, last] {
        assert_eq!(
            query(&f.doc, json!({"query":"dependents","uid":feature})),
            json!({"dependents": []})
        );
    }
    // The excluded component's last feature deletes alone; the cut keeps
    // working and keeps its exclusion, as Part:1 is still there.
    let reply = command(&mut f.doc, json!({"cmd":"delete_feature","uid":last}));
    assert_eq!(reply["deleted"], json!([last]));
    assert!(!reply.to_string().contains("exclusion"), "{reply}");
    assert!(f.doc.feature(last).is_none());
    assert_eq!(f.doc.status(f.cut), Some(&FeatureStatus::Ok));
    assert!(is_cut(&f.doc, f.holder));
    assert!(!is_cut(&f.doc, f.part[0]));
    assert_eq!(scope_of(&f.doc, f.cut).stored_targets().len(), 1);
    // With its dependents, a delete takes no feature of the other component.
    f.doc.undo();
    let deleted = f.doc.delete_feature(f.part[0].feature, true).unwrap();
    assert_eq!(deleted, vec![f.part[0].feature]);
    assert_eq!(f.doc.status(f.cut), Some(&FeatureStatus::Ok));
    assert!(is_cut(&f.doc, f.holder));
    assert!(!is_cut(&f.doc, f.part[1]));
    // Nor does suppressing it, or moving it after the cut, make the cut
    // fail.
    f.doc.undo();
    f.doc.set_suppressed(f.part[0].feature, true).unwrap();
    assert_eq!(f.doc.status(f.cut), Some(&FeatureStatus::Ok));
    f.doc.undo();
    let end = f.doc.features().count() - 1;
    f.doc.move_feature(f.part[0].feature, end).unwrap();
    assert_eq!(f.doc.status(f.cut), Some(&FeatureStatus::Ok));
    assert!(is_cut(&f.doc, f.holder));
}

#[test]
fn deleting_an_excluded_occurrence_drops_the_exclusion_with_a_notice() {
    let mut f = excluding();
    let reply = command(
        &mut f.doc,
        json!({"cmd":"delete_occurrence","occurrence":"O1"}),
    );
    // Part's features go with its last occurrence; the cut stays.
    assert_eq!(
        reply["deleted"],
        json!([f.part[0].feature, f.part[1].feature])
    );
    let notices = reply["notices"].as_array().unwrap();
    let notice = notices
        .iter()
        .find(|n| n["target"] == json!(f.cut))
        .unwrap_or_else(|| panic!("{reply}"));
    assert_eq!(notice["level"], "info");
    let message = notice["message"].as_str().unwrap();
    assert!(
        message.contains("Part:1") && message.contains("dropped"),
        "{message}"
    );
    assert_eq!(f.doc.status(f.cut), Some(&FeatureStatus::Ok));
    assert!(is_cut(&f.doc, f.holder));
    assert!(scope_of(&f.doc, f.cut).stored_targets().is_empty());
    let saved = f.doc.to_json();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(reopened.status(f.cut), Some(&FeatureStatus::Ok));
    // Undo brings the occurrence and the exclusion back.
    f.doc.undo();
    assert_eq!(scope_of(&f.doc, f.cut).stored_targets().len(), 1);
    assert_eq!(f.doc.status(f.cut), Some(&FeatureStatus::Ok));
    assert!(f.part.iter().all(|b| !is_cut(&f.doc, *b)));
}

#[test]
fn deleting_an_excluded_body_drops_the_exclusion_with_a_notice() {
    let mut f = excluding();
    f.definition["cut_scope"]["targets"] = json!({"type":"automatic",
        "excluded_bodies":[{"component":"C1","occurrence":"O1","body":f.part[1]}]});
    f.doc
        .edit_feature(f.cut, &cut_definition(&f.definition))
        .unwrap();
    assert_eq!(f.doc.status(f.cut), Some(&FeatureStatus::Ok));
    assert!(is_cut(&f.doc, f.part[0]));
    assert!(!is_cut(&f.doc, f.part[1]));
    // The excluded body's maker has no dependents, and suppressing it
    // leaves the cut working.
    let maker = f.part[1].feature;
    assert_eq!(
        query(&f.doc, json!({"query":"dependents","uid":maker})),
        json!({"dependents": []})
    );
    f.doc.set_suppressed(maker, true).unwrap();
    assert_eq!(f.doc.status(f.cut), Some(&FeatureStatus::Ok));
    f.doc.undo();
    let reply = command(&mut f.doc, json!({"cmd":"delete_feature","uid":maker}));
    assert_eq!(reply["deleted"], json!([maker]));
    let notice = reply["notices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["target"] == json!(f.cut))
        .unwrap_or_else(|| panic!("{reply}"));
    assert_eq!(notice["level"], "info");
    assert!(
        notice["message"].as_str().unwrap().contains("dropped"),
        "{notice}"
    );
    assert!(scope_of(&f.doc, f.cut).excluded_bodies().is_empty());
    assert_eq!(f.doc.status(f.cut), Some(&FeatureStatus::Ok));
    assert!(is_cut(&f.doc, f.part[0]));
    assert!(is_cut(&f.doc, f.holder));
}

#[test]
fn cut_scope_exclusions_survive_target_moving_out_of_and_back_into_tool() {
    let mut f = fixture();
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    f.doc
        .set_occurrence_transform(
            OccurrenceUid(2),
            Transform::translation([100.0, 0.0, 0.0]),
            false,
        )
        .unwrap();
    let report = f.doc.preview_edit(uid, &cut_definition(&f.cut)).unwrap();
    let targets = report.cut_targets.unwrap();
    let b = targets
        .rows
        .iter()
        .find(|r| r.component == ComponentUid(2))
        .unwrap();
    assert!(!b.intersects && !b.checked);
    f.doc
        .set_occurrence_transform(
            OccurrenceUid(2),
            Transform::translation([0.0, 0.0, 20.0]),
            false,
        )
        .unwrap();
    assert!(!is_cut(&f.doc, f.bodies[1]));
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
}

#[test]
fn cut_scope_rejects_ambiguous_context_and_mixed_legacy_participants() {
    let mut f = fixture();
    f.cut["participants"] = json!([f.bodies[0]]);
    assert!(f.doc.add_feature(&cut_definition(&f.cut), None).is_err());
    f.cut.as_object_mut().unwrap().remove("participants");
    f.cut["cut_scope"]["context"]["component"] = json!("C1");
    assert!(f.doc.add_feature(&cut_definition(&f.cut), None).is_err());
}

#[test]
fn cut_scope_linked_target_is_disabled_and_never_modified() {
    let f = fixture();
    let mut state = f.doc.state().clone();
    state.assembly.component_mut(ComponentUid(1)).unwrap().link =
        Some(mitcad_model::assembly::ExternalLink {
            path: "synthetic-part.mitcad".to_owned(),
            digest: String::new(),
            feature: f.bodies[0].feature,
            envelope: None,
            frames: Vec::new(),
        });
    let mut doc = Document::from_state(MockKernel::default(), state);
    doc.recompute();
    let report = doc.preview_add(&cut_definition(&f.cut), None).unwrap();
    assert_eq!(report.status, FeatureStatus::Ok);
    let targets = report.cut_targets.unwrap();
    let a = targets
        .rows
        .iter()
        .find(|r| r.component == ComponentUid(1))
        .unwrap();
    assert!(!a.eligible && !a.checked);
    assert!(a.reason.as_deref().unwrap().contains("read-only"));
    doc.add_feature(&cut_definition(&f.cut), None).unwrap();
    assert!(!is_cut(&doc, f.bodies[0]));
    assert!(is_cut(&doc, f.bodies[2]));
}

#[test]
fn cut_scope_root_body_has_explicit_root_ownership() {
    let mut f = fixture();
    let root = box_body(&mut f.doc);
    f.cut["cut_scope"]["targets"] =
        json!({"type":"explicit","included":[{"component":"C0","occurrence":""}]});
    let report = f.doc.preview_add(&cut_definition(&f.cut), None).unwrap();
    let targets = report.cut_targets.unwrap();
    assert_eq!(targets.rows[0].component, ComponentUid::ROOT);
    assert_eq!(targets.rows[0].path_name, "Root");
    assert_eq!(targets.selected_bodies, 1);
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    assert_eq!(f.doc.body_component(root), Some(ComponentUid::ROOT));
    assert!(is_cut(&f.doc, root));
    assert!(f.bodies.iter().all(|b| !is_cut(&f.doc, *b)));
}

#[test]
fn cut_scope_disk_cache_restores_owner_overrides_and_preview_metadata() {
    use mitcad_model::store::ResultStore;
    use std::time::Duration;
    let mut f = fixture();
    f.doc.kernel().split.set(Some(2));
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    let dir = mitcad_model::testing::unique_temp_path("cut-scope-store");
    f.doc
        .set_result_store(Some(ResultStore::new(&dir, "cut-scope-test")));
    let persisted = f.doc.persist_results(Duration::ZERO);
    assert_eq!(persisted.failed, 0);
    let mut opened = Document::from_json(&f.doc.to_json(), MockKernel::default()).unwrap();
    opened.set_result_store(Some(ResultStore::new(&dir, "cut-scope-test")));
    opened.recompute();
    assert!(opened.stats().restored.contains(&uid));
    assert_eq!(
        opened.body_component(BodyUid::new(uid, 0)),
        Some(ComponentUid(1))
    );
    assert_eq!(
        opened.body_component(BodyUid::new(uid, 1)),
        Some(ComponentUid(3))
    );
    let report = opened.preview_edit(uid, &cut_definition(&f.cut)).unwrap();
    assert_eq!(report.cut_targets.unwrap().selected_components, 2);
    mitcad_model::testing::remove_temp(&dir);
}

#[test]
fn cut_scope_excluded_body_failure_never_skips_other_requested_bodies() {
    let mut f = fixture();
    f.doc.activate_component(ComponentUid(1)).unwrap();
    let excluded = box_body(&mut f.doc);
    f.doc.activate_component(ComponentUid::ROOT).unwrap();
    let history = f.doc.body_shape(excluded).unwrap().history.clone();
    f.doc
        .kernel()
        .boolean_fail_targets
        .borrow_mut()
        .insert(history);
    f.cut["cut_scope"]["targets"]["excluded_bodies"] = json!([
        {"component":"C1","occurrence":"O1","body":excluded}]);
    let report = f.doc.preview_add(&cut_definition(&f.cut), None).unwrap();
    assert_eq!(report.status, FeatureStatus::Ok);
    let targets = report.cut_targets.unwrap();
    assert!(targets.can_commit);
    assert_eq!(targets.selected_bodies, 2);
    assert!(targets.rows[0].eligible && targets.rows[0].partial);
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    assert!(is_cut(&f.doc, f.bodies[0]));
    assert!(is_cut(&f.doc, f.bodies[2]));
    assert!(!is_cut(&f.doc, excluded));
    // Including that same failed body rejects the entire change, including C.
    f.cut["cut_scope"]["targets"]["excluded_bodies"] = json!([]);
    f.doc.edit_feature(uid, &cut_definition(&f.cut)).unwrap();
    assert!(matches!(f.doc.status(uid), Some(FeatureStatus::Failed(_))));
    assert!(f.bodies.iter().all(|b| !is_cut(&f.doc, *b)));
}

#[test]
fn cut_scope_deleting_an_excluded_definition_keeps_the_cut_and_undo_restores_the_exclusion() {
    let mut f = fixture();
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    // The exclusion is no reference (mitcad#1236): the cut stays, without
    // the exclusion of what is gone.
    let deleted = f.doc.delete_occurrence(OccurrenceUid(2)).unwrap();
    assert_eq!(deleted, vec![f.bodies[1].feature]);
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    assert!(scope_of(&f.doc, uid).stored_targets().is_empty());
    assert!(is_cut(&f.doc, f.bodies[0]));
    assert!(is_cut(&f.doc, f.bodies[2]));
    f.doc.undo();
    assert_eq!(scope_of(&f.doc, uid).stored_targets().len(), 1);
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    assert!(is_cut(&f.doc, f.bodies[0]));
    assert!(!is_cut(&f.doc, f.bodies[1]));
    assert!(is_cut(&f.doc, f.bodies[2]));
    let mut reopened = Document::from_json(&f.doc.to_json(), MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(reopened.status(uid), Some(&FeatureStatus::Ok));
    assert!(!is_cut(&reopened, f.bodies[1]));
}

#[test]
fn cut_scope_paste_new_remaps_internal_target_and_source_context() {
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("Part"), Transform::IDENTITY, true)
        .unwrap();
    let original = box_body(&mut doc);
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    doc.add_feature(
        &def(
            json!({"type":"extrude", "profiles":[{"sketch":sketch,"region":region}],
        "extent":{"type":"distance","distance":10}, "operation":"cut",
        "cut_scope":{"context":{"component":"C1","occurrence":"O1"},
            "targets":{"type":"explicit","included":[{"component":"C1","occurrence":"O1"}]}}}),
        ),
        None,
    )
    .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (component, occurrence) = doc
        .paste_new(
            OccurrenceUid(1),
            Some(Transform::translation([50.0, 0.0, 0.0])),
        )
        .unwrap();
    let copied = doc
        .features()
        .find(|f| f.component == component && f.def.type_name() == "extrude")
        .unwrap();
    assert_eq!(doc.status(copied.uid), Some(&FeatureStatus::Ok));
    let FeatureDef::Extrude(extrude) = &copied.def else {
        panic!("copied cut");
    };
    let scope = extrude.cut_scope.as_ref().unwrap();
    assert_eq!(scope.context.component, component);
    assert_eq!(scope.context.occurrence.0, vec![occurrence]);
    assert_eq!(scope.stored_targets(), std::slice::from_ref(&scope.context));
    let bodies = doc.component_bodies(component);
    assert_eq!(bodies.len(), 1);
    assert_ne!(bodies[0], original);
    assert!(is_cut(&doc, bodies[0]));
    assert!(is_cut(&doc, original));
}

#[test]
fn dormant_cut_scope_does_not_change_boss_geometry_references_or_replay() {
    for operation in ["join", "new_body", "intersect", "new_component"] {
        let mut f = fixture();
        // Intersect needs a local participant; the other operations work
        // without one. A dormant scope must never read foreign targets.
        f.doc.activate_component(ComponentUid(1)).unwrap();
        let sketch = f.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
        let region = f
            .doc
            .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
            .unwrap()
            .region;
        f.cut["profiles"] = json!([{"sketch":sketch,"region":region}]);
        f.cut["operation"] = json!(operation);
        f.cut["cut_scope"]["context"]["component"] = json!("C1");
        f.cut["cut_scope"]["context"]["occurrence"] = json!("O1");
        let with_scope = cut_definition(&f.cut);
        let mut plain = f.cut.clone();
        plain.as_object_mut().unwrap().remove("cut_scope");
        let without_scope = cut_definition(&plain);
        let before = f.doc.to_json();
        let mut control = Document::from_json(&before, MockKernel::default()).unwrap();
        control.recompute();
        let uid = f.doc.add_feature(&with_scope, None).unwrap().uid;
        control.add_feature(&without_scope, None).unwrap();
        let stored = &f
            .doc
            .features()
            .find(|feature| feature.uid == uid)
            .unwrap()
            .def;
        let plain_stored = &control
            .features()
            .find(|feature| feature.uid == uid)
            .unwrap()
            .def;
        assert_eq!(stored.references(), plain_stored.references());
        assert_eq!(stored.info().tool_use(), plain_stored.info().tool_use());
        assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
        for body in &f.bodies {
            assert_eq!(f.doc.body_shape(*body), control.body_shape(*body));
        }
        let saved = f.doc.to_json();
        let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
        reopened.recompute();
        assert_eq!(reopened.to_json(), saved);
        assert_eq!(reopened.status(uid), Some(&FeatureStatus::Ok));
    }
}

#[test]
fn scoped_cut_join_cut_restores_exclusions_through_undo_and_reopen() {
    let mut f = fixture();
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    let mut boss = f.cut.clone();
    boss["operation"] = json!("new_body");
    f.doc.edit_feature(uid, &cut_definition(&boss)).unwrap();
    assert!(f.bodies.iter().all(|body| !is_cut(&f.doc, *body)));
    let saved = f.doc.to_json();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.recompute();
    reopened.edit_feature(uid, &cut_definition(&f.cut)).unwrap();
    assert_eq!(reopened.status(uid), Some(&FeatureStatus::Ok));
    for (i, body) in f.bodies.iter().enumerate() {
        assert_eq!(is_cut(&reopened, *body), i != 1);
    }
    let cut_saved = reopened.to_json();
    reopened.undo();
    assert!(f.bodies.iter().all(|body| !is_cut(&reopened, *body)));
    reopened.redo();
    assert_eq!(reopened.to_json(), cut_saved);
}

#[test]
fn unresolved_modern_cut_scope_is_dormant_until_editor_conversion() {
    let mut f = fixture();
    f.cut["cut_scope"]["initialize_on_cut"] = json!(true);
    // Resolving visibility is an editor action; an API Cut must not silently
    // activate an unresolved boss's automatic target scope.
    let error = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap_err();
    assert!(error.to_string().contains("initialize_on_cut"));
    f.cut["operation"] = json!("new_body");
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    assert!(f.bodies.iter().all(|body| !is_cut(&f.doc, *body)));
    let saved = f.doc.to_json();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(reopened.to_json(), saved);
    f.cut["operation"] = json!("cut");
    f.cut["cut_scope"]
        .as_object_mut()
        .unwrap()
        .remove("initialize_on_cut");
    reopened.edit_feature(uid, &cut_definition(&f.cut)).unwrap();
    assert!(is_cut(&reopened, f.bodies[0]));
    assert!(!is_cut(&reopened, f.bodies[1]));
    assert!(is_cut(&reopened, f.bodies[2]));
}

#[test]
fn dormant_cut_choices_follow_reparent_without_active_geometry_references() {
    let mut f = fixture();
    f.doc.activate_component(ComponentUid(1)).unwrap();
    let excluded_body = box_body(&mut f.doc);
    let sketch = f.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = f
        .doc
        .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    f.cut["profiles"] = json!([{"sketch":sketch,"region":region}]);
    f.cut["cut_scope"] = json!({"context":{"component":"C1","occurrence":"O1"},
        "targets":{"type":"automatic",
            "excluded":[{"component":"C2","occurrence":"O2"},{"component":"C3","occurrence":"O3"}],
            "excluded_bodies":[{"component":"C1","occurrence":"O1","body":excluded_body}]}});
    let uid = f
        .doc
        .add_feature(&cut_definition(&f.cut), None)
        .unwrap()
        .uid;
    assert!(is_cut(&f.doc, f.bodies[0]));
    assert!(!is_cut(&f.doc, excluded_body));
    let mut join = f.cut.clone();
    join["operation"] = json!("join");
    f.doc.edit_feature(uid, &cut_definition(&join)).unwrap();
    let entry = f.doc.features().find(|feature| feature.uid == uid).unwrap();
    assert!(mitcad_model::internal::joints::referenced_occurrences(&entry.def).is_empty());
    f.doc.activate_component(ComponentUid::ROOT).unwrap();
    f.doc
        .command(&json!({"cmd":"reparent_occurrence","occurrence":"O1","parent":"O2"}).to_string())
        .unwrap();
    let entry = f.doc.features().find(|feature| feature.uid == uid).unwrap();
    let FeatureDef::Extrude(extrude) = &entry.def else {
        panic!("expected extrusion")
    };
    let scope = extrude.cut_scope.as_ref().unwrap();
    assert_eq!(scope.context.occurrence.to_string(), "O2/O1");
    assert_eq!(scope.excluded_bodies()[0].occurrence.to_string(), "O2/O1");
    assert!(mitcad_model::internal::joints::referenced_occurrences(&entry.def).is_empty());
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    let saved = f.doc.to_json();
    f.doc.undo();
    f.doc.redo();
    assert_eq!(f.doc.to_json(), saved);
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.recompute();
    f.cut["cut_scope"]["context"]["occurrence"] = json!("O2/O1");
    f.cut["cut_scope"]["targets"]["excluded_bodies"][0]["occurrence"] = json!("O2/O1");
    reopened.edit_feature(uid, &cut_definition(&f.cut)).unwrap();
    assert_eq!(reopened.status(uid), Some(&FeatureStatus::Ok));
    assert!(is_cut(&reopened, f.bodies[0]));
    assert!(!is_cut(&reopened, excluded_body));
    assert!(!is_cut(&reopened, f.bodies[1]));
    assert!(!is_cut(&reopened, f.bodies[2]));
}

/// mitcad#769: Ring (C1) placed twice (O1 shown, O3 at x 100 hidden) with
/// its own block; Other (C2) placed twice (O2 20 up over Ring:1, O4 far
/// away) with a block. The sketch is Ring's own, a square inside its block.
struct SharedFixture {
    doc: Document<MockKernel>,
    ring: BodyUid,
    other: BodyUid,
    sketch: FeatureUid,
    region: Value,
}

fn shared_fixture() -> SharedFixture {
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("Ring"), Transform::IDENTITY, true)
        .unwrap();
    let ring = box_body(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.create_component(
        Some("Other"),
        Transform::translation([0.0, 0.0, 20.0]),
        true,
    )
    .unwrap();
    let other = box_body(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let ring_copy = doc
        .copy_occurrence(
            OccurrenceUid(1),
            Some(Transform::translation([100.0, 0.0, 0.0])),
        )
        .unwrap();
    assert_eq!(ring_copy, OccurrenceUid(3));
    doc.set_occurrence_visible(ring_copy, false).unwrap();
    let other_copy = doc
        .copy_occurrence(
            OccurrenceUid(2),
            Some(Transform::translation([200.0, 0.0, 20.0])),
        )
        .unwrap();
    assert_eq!(other_copy, OccurrenceUid(4));
    doc.activate_component(ComponentUid(1)).unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let region = serde_json::to_value(region).unwrap();
    SharedFixture {
        doc,
        ring,
        other,
        sketch,
        region,
    }
}

impl SharedFixture {
    fn cut(&self, context: Value, targets: Value) -> Value {
        json!({"type":"extrude",
            "profiles":[{"sketch":self.sketch,"region":self.region}],
            "extent":{"type":"distance","distance":60}, "operation":"cut",
            "cut_scope":{"context":context, "targets":targets}})
    }
}

#[test]
fn cut_inside_a_component_with_several_occurrences_cuts_its_own_definition() {
    let mut f = shared_fixture();
    assert_eq!(f.doc.assembly().paths_to(ComponentUid(1)).len(), 2);
    let cut = f.cut(
        json!({"component":"C1","occurrence":"O1"}),
        json!({"type":"automatic","excluded":[]}),
    );
    let report = f.doc.preview_add(&cut_definition(&cut), None).unwrap();
    assert_eq!(report.status, FeatureStatus::Ok);
    let targets = report.cut_targets.unwrap();
    assert!(targets.can_commit);
    assert_eq!(
        (targets.selected_components, targets.selected_bodies),
        (1, 1)
    );
    // Ring is one row, at the context occurrence: eligible, checked and
    // saying that every occurrence changes. Its hidden second occurrence
    // is no row of its own.
    let ring = targets
        .rows
        .iter()
        .find(|r| r.component == ComponentUid(1))
        .unwrap();
    assert_eq!(ring.occurrence.to_string(), "O1");
    assert!(ring.eligible && ring.checked && ring.intersects);
    assert!(ring.reason.is_none());
    assert!(
        ring.note.as_deref().unwrap().contains("all 2 occurrences"),
        "{:?}",
        ring.note
    );
    assert_eq!(
        targets
            .rows
            .iter()
            .filter(|r| r.component == ComponentUid(1))
            .count(),
        1
    );
    // Other's shared definition, reached from outside it, stays refused.
    let other = targets
        .rows
        .iter()
        .find(|r| r.component == ComponentUid(2))
        .unwrap();
    assert_eq!(other.occurrence.to_string(), "O2");
    assert!(!other.eligible && !other.checked && other.note.is_none());
    assert!(
        other
            .reason
            .as_deref()
            .unwrap()
            .contains("multiple occurrences")
    );
    assert!(!is_cut(&f.doc, f.ring), "preview cannot mutate bodies");

    // Over the JSON command, as the panel commits it.
    f.doc
        .command(&json!({"cmd":"add_feature","def":cut}).to_string())
        .unwrap();
    let uid = f
        .doc
        .features()
        .find(|feature| feature.def.type_name() == "extrude")
        .unwrap()
        .uid;
    assert_eq!(f.doc.status(uid), Some(&FeatureStatus::Ok));
    // The definition's body has the groove; both occurrences place that one
    // body, so both show it. Other is unchanged.
    assert!(is_cut(&f.doc, f.ring));
    assert_eq!(f.doc.body_component(f.ring), Some(ComponentUid(1)));
    assert_eq!(f.doc.component_bodies(ComponentUid(1)), vec![f.ring]);
    assert_eq!(f.doc.assembly().occurrences_of(ComponentUid(1)).count(), 2);
    assert!(!is_cut(&f.doc, f.other));

    let saved = f.doc.to_json();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    reopened.recompute();
    assert_eq!(reopened.status(uid), Some(&FeatureStatus::Ok));
    assert!(is_cut(&reopened, f.ring));
    assert!(!is_cut(&reopened, f.other));
    f.doc.undo();
    assert!(!is_cut(&f.doc, f.ring));
}

#[test]
fn own_component_with_one_occurrence_has_no_note() {
    let mut f = shared_fixture();
    f.doc.activate_component(ComponentUid::ROOT).unwrap();
    f.doc.delete_occurrence(OccurrenceUid(3)).unwrap();
    f.doc.activate_component(ComponentUid(1)).unwrap();
    let cut = f.cut(
        json!({"component":"C1","occurrence":"O1"}),
        json!({"type":"automatic","excluded":[]}),
    );
    let report = f.doc.preview_add(&cut_definition(&cut), None).unwrap();
    assert_eq!(report.status, FeatureStatus::Ok);
    let targets = report.cut_targets.unwrap();
    let ring = targets
        .rows
        .iter()
        .find(|r| r.component == ComponentUid(1))
        .unwrap();
    assert!(ring.eligible && ring.checked && ring.note.is_none());
}

#[test]
fn own_component_included_at_another_occurrence_fails() {
    let mut f = shared_fixture();
    // The own definition is one target, at the context occurrence; naming
    // its other occurrence would be an occurrence-specific cut.
    let cut = f.cut(
        json!({"component":"C1","occurrence":"O1"}),
        json!({"type":"explicit","included":[{"component":"C1","occurrence":"O3"}]}),
    );
    let report = f.doc.preview_add(&cut_definition(&cut), None).unwrap();
    assert!(
        matches!(report.status, FeatureStatus::Failed(ref message) if message.contains("own definition")),
        "{:?}",
        report.status
    );
    assert!(!report.cut_targets.unwrap().can_commit);
    let cut = f.cut(
        json!({"component":"C1","occurrence":"O1"}),
        json!({"type":"explicit","included":[{"component":"C1","occurrence":"O1"}]}),
    );
    let report = f.doc.preview_add(&cut_definition(&cut), None).unwrap();
    assert_eq!(report.status, FeatureStatus::Ok);
    assert_eq!(report.cut_targets.unwrap().selected_bodies, 1);
}

#[test]
fn cut_from_outside_still_refuses_one_occurrence_of_a_shared_component() {
    let mut f = shared_fixture();
    // The same square, sketched in the root: it reaches Ring:1 only, and a
    // change of Ring's definition would change its hidden Ring:2 as well.
    f.doc.activate_component(ComponentUid::ROOT).unwrap();
    let sketch = f.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = f
        .doc
        .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    f.sketch = sketch;
    f.region = serde_json::to_value(region).unwrap();
    let cut = f.cut(
        json!({"component":"C0","occurrence":""}),
        json!({"type":"automatic","excluded":[]}),
    );
    let report = f.doc.preview_add(&cut_definition(&cut), None).unwrap();
    assert!(
        matches!(report.status, FeatureStatus::Failed(ref message) if message.contains("No eligible components")),
        "{:?}",
        report.status
    );
    let targets = report.cut_targets.unwrap();
    assert!(!targets.can_commit);
    for component in [ComponentUid(1), ComponentUid(2)] {
        let row = targets
            .rows
            .iter()
            .find(|r| r.component == component)
            .unwrap();
        assert!(!row.eligible && !row.checked && row.note.is_none());
        assert!(
            row.reason
                .as_deref()
                .unwrap()
                .contains("multiple occurrences")
        );
    }
    f.doc.add_feature(&cut_definition(&cut), None).unwrap();
    assert!(!is_cut(&f.doc, f.ring));
    assert!(!is_cut(&f.doc, f.other));
}

/// A component whose box's top face lies in the root's XY plane, and a cut
/// of a 4 mm square sketched there at `corner`, 5 mm along +Z (out of the
/// box), `flip` reversing it. The mock cuts only bodies its tool overlaps
/// by volume, so a tool that only meets the top face misses.
fn top_face_cut(corner: [f64; 2], flip: bool) -> (Document<MockKernel>, BodyUid, Value) {
    let kernel = MockKernel::default();
    kernel.cuts_by_bounds.set(true);
    let mut doc = Document::new(kernel);
    doc.create_component(Some("A"), Transform::translation([0.0, 0.0, -10.0]), true)
        .unwrap();
    let body = box_body(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, corner, &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let cut = json!({"type":"extrude",
        "profiles":[{"sketch":sketch,"region":region}], "extent":{"type":"distance","distance":5},
        "flip":flip, "operation":"cut", "cut_scope":{"context":{"component":"C0","occurrence":""},
            "targets":{"type":"automatic","excluded":[]}}});
    (doc, body, cut)
}

#[test]
fn cut_out_of_the_body_suggests_flip_when_flipped_it_cuts() {
    // Out of the top face: nothing is cut, and Flip would cut the box.
    let (mut doc, body, cut) = top_face_cut([2.0, 2.0], false);
    let report = doc.preview_add(&cut_definition(&cut), None).unwrap();
    assert!(
        matches!(report.status, FeatureStatus::Failed(ref message)
            if message.starts_with("No eligible components intersect the cut") && message.contains("Try Flip")),
        "{:?}",
        report.status
    );
    // The report says so for the panel, and is otherwise the miss's own.
    let targets = report.cut_targets.unwrap();
    assert!(targets.flip_would_cut);
    assert_eq!(
        serde_json::to_value(&targets).unwrap()["flip_would_cut"],
        json!(true)
    );
    assert!(!targets.can_commit);
    assert_eq!(targets.selected_bodies, 0);
    assert!(targets.rows.iter().all(|r| !r.intersects));
    doc.add_feature(&cut_definition(&cut), None).unwrap();
    assert!(!is_cut(&doc, body));

    // Flipped, the same cut goes into the box and cuts it.
    let (mut doc, body, cut) = top_face_cut([2.0, 2.0], true);
    let report = doc.preview_add(&cut_definition(&cut), None).unwrap();
    assert_eq!(report.status, FeatureStatus::Ok);
    let targets = report.cut_targets.unwrap();
    assert!(targets.can_commit && !targets.flip_would_cut);
    doc.add_feature(&cut_definition(&cut), None).unwrap();
    assert!(is_cut(&doc, body));
}

#[test]
fn cut_that_misses_both_ways_does_not_suggest_flip() {
    // Beside the box: neither direction reaches it.
    for flip in [false, true] {
        let (mut doc, body, cut) = top_face_cut([50.0, 50.0], flip);
        let report = doc.preview_add(&cut_definition(&cut), None).unwrap();
        assert!(
            matches!(report.status, FeatureStatus::Failed(ref message)
                if message == "No eligible components intersect the cut"),
            "{:?}",
            report.status
        );
        let targets = report.cut_targets.unwrap();
        assert!(!targets.can_commit && !targets.flip_would_cut);
        // Left out of the report when false.
        assert!(
            serde_json::to_value(&targets)
                .unwrap()
                .get("flip_would_cut")
                .is_none()
        );
        doc.add_feature(&cut_definition(&cut), None).unwrap();
        assert!(!is_cut(&doc, body));
    }
}
