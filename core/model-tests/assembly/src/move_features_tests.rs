// SPDX-License-Identifier: MIT
//! Move Features into a Component (mitcad#171) with the mock kernel: the
//! root's modelled features go into a new component where they were,
//! references follow, a feature that ties bodies of different components
//! refuses a split unless they stay together, the history of a component
//! made from bodies moves into it, and each move is one undo step.

use serde_json::{Value, json};

use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::test_support::{block, def, extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, FeatureStatus, MoveGroup};
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string()).unwrap_err().to_string()
}

/// Every visible body as placed in the design: its id and box.
fn placed(doc: &Document<MockKernel>) -> Vec<(BodyUid, BoundingBox)> {
    let mut out: Vec<(BodyUid, BoundingBox)> = doc
        .instances()
        .into_iter()
        .filter(|i| i.visible)
        .map(|i| {
            let shape = doc.body_shape(i.body).unwrap();
            let moved = doc
                .kernel()
                .transform_shape(shape, &i.transform, None)
                .unwrap();
            (i.body, moved.bounds.unwrap())
        })
        .collect();
    out.sort_by_key(|(b, _)| *b);
    out
}

fn components(doc: &Document<MockKernel>) -> Vec<ComponentUid> {
    doc.features().map(|f| f.component).collect()
}

fn all_ok(doc: &Document<MockKernel>) {
    for item in doc.timeline() {
        assert_eq!(
            item.status,
            Some(&FeatureStatus::Ok),
            "{}: {:?}",
            item.entry.name,
            item.status
        );
    }
}

/// A 10 x 10 x 5 block from a sketch of its own at x, as Sketch and
/// Extrude of the active component.
fn small_block(doc: &mut Document<MockKernel>, x: f64) -> (FeatureUid, BodyUid) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [x, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (uid, BodyUid::new(uid, 0))
}

#[test]
fn everything_goes_into_one_component_where_it_was() {
    let mut b = block();
    // A fillet on the block, a second body, and a user parameter the second
    // extrude uses.
    let fillet = b
        .doc
        .add_feature(
            &def(json!({"type": "fillet", "body": b.body,
                        "edges": [format!("E{{{0}:side(c1[c4,c2])|{0}:side(c2[c1,c3])}}", b.extrude)],
                        "radius": 1})),
            None,
        )
        .unwrap()
        .uid;
    b.doc.add_parameter("height", 7.0, "").unwrap();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, [100.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let second = b
        .doc
        .add_feature(
            &def(
                json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                        "extent": {"type": "distance", "distance": "height"},
                        "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    let before = placed(&b.doc);
    let order: Vec<FeatureUid> = b.doc.features().map(|f| f.uid).collect();
    let parameters: Vec<String> = b
        .doc
        .parameters()
        .iter()
        .map(|p| p.name().to_owned())
        .collect();
    let depth = b.doc.undo_depth();

    // The plan: every body of the root, every feature.
    let plan = query(&b.doc, json!({"query": "move_to_component"}));
    assert_eq!(plan["everything"], true);
    assert_eq!(plan["bodies"].as_array().unwrap().len(), 2);
    assert_eq!(plan["groups"][0]["features"].as_array().unwrap().len(), 5);
    assert_eq!(plan["conflicts"], json!([]));
    assert_eq!(plan["hybrid_assembly"], false);

    let moved = command(&mut b.doc, json!({"cmd": "move_to_component"}));
    let made = &moved["components"][0];
    assert_eq!(made["name"], "Component1");
    let component: ComponentUid = made["component"].as_str().unwrap().parse().unwrap();
    // One undo step; the features keep their order, uids and parameters,
    // the root keeps no body, and nothing moved.
    assert_eq!(b.doc.undo_depth(), depth + 1);
    assert_eq!(b.doc.features().map(|f| f.uid).collect::<Vec<_>>(), order);
    assert!(components(&b.doc).iter().all(|c| *c == component));
    assert_eq!(
        b.doc
            .parameters()
            .iter()
            .map(|p| p.name().to_owned())
            .collect::<Vec<_>>(),
        parameters
    );
    assert!(b.doc.component_bodies(ComponentUid::ROOT).is_empty());
    assert_eq!(
        b.doc.body_component(BodyUid::new(second, 0)),
        Some(component)
    );
    assert_eq!(placed(&b.doc), before);
    all_ok(&b.doc);
    // The new component's occurrence is grounded at the identity.
    let occurrence = b
        .doc
        .assembly()
        .occurrences_of(component)
        .next()
        .unwrap()
        .clone();
    assert!(occurrence.grounded && occurrence.transform.is_identity());
    assert_eq!(occurrence.parent, ComponentUid::ROOT);
    assert_eq!(
        b.doc.undo_label(),
        Some("Move Features of Root into Component1")
    );
    // The features still work in their component: the parameter drives the
    // second body.
    b.doc.set_parameter("height", 9.0).unwrap();
    let top = b
        .doc
        .body_shape(BodyUid::new(second, 0))
        .unwrap()
        .bounds
        .unwrap()
        .max[2];
    assert!((top - 9.0).abs() < 1e-9, "{top}");
    assert_eq!(b.doc.status(fillet), Some(&FeatureStatus::Ok));
    b.doc.undo();

    // The design file keeps it.
    let saved = b.doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(placed(&loaded), before);
    assert_eq!(components(&loaded), components(&b.doc));

    // Undo takes it all back at once; redo brings it again.
    b.doc.undo();
    assert!(components(&b.doc).iter().all(|c| c.is_root()));
    assert!(b.doc.assembly().components.is_empty());
    assert_eq!(b.doc.component_bodies(ComponentUid::ROOT).len(), 2);
    assert_eq!(placed(&b.doc), before);
    b.doc.redo();
    assert!(components(&b.doc).iter().all(|c| *c == component));

    // Nothing is left to move.
    let error = rejected(&mut b.doc, json!({"cmd": "move_to_component"}));
    assert!(error.contains("no modeled features"), "{error}");
}

#[test]
fn references_from_other_components_follow_the_new_occurrence() {
    let mut b = block();
    // B:1 (O1) beside the root's block, with a sketch on the block's top
    // face picked where the root shows it, and a linked projection of it.
    let (other, _) = b
        .doc
        .create_component(Some("B"), Transform::translation([0.0, 50.0, -3.0]), true)
        .unwrap();
    mitcad_model::test_support::allow_attachments(&mut b.doc);
    let top = format!("{}:end({})", b.extrude, b.region);
    let sketch = command(
        &mut b.doc,
        json!({"cmd": "sketch.create", "plane": {"face": top, "body": b.body}, "occurrence": ""}),
    )["uid"]
        .clone();
    let uid: FeatureUid = sketch.as_str().unwrap().parse().unwrap();
    assert_eq!(b.doc.feature(uid).unwrap().component, other);
    let frame = |doc: &Document<MockKernel>| {
        query(doc, json!({"query": "sketch", "uid": sketch}))["frame"].clone()
    };
    let before = frame(&b.doc);
    assert!(before.is_object(), "{:?}", b.doc.status(uid));
    // A later feature of the root names the block's faces.
    b.doc.activate_component(ComponentUid::ROOT).unwrap();
    let fillet = b
        .doc
        .add_feature(
            &def(json!({"type": "fillet", "body": b.body,
                        "edges": [format!("E{{{0}:side(c1[c4,c2])|{0}:side(c2[c1,c3])}}", b.extrude)],
                        "radius": 1})),
            None,
        )
        .unwrap()
        .uid;
    // The view isolated to the root's block.
    command(
        &mut b.doc,
        json!({"cmd": "set_isolation", "items": [{"body": b.body}]}),
    );

    command(
        &mut b.doc,
        json!({"cmd": "move_to_component", "name": "Base"}),
    );
    let base = b.doc.feature(b.extrude).unwrap().component;
    assert_eq!(b.doc.assembly().name(base), "Base");
    // B's sketch is still B's, its plane now seen through Base:1 (O2).
    assert_eq!(b.doc.feature(uid).unwrap().component, other);
    let s = query(&b.doc, json!({"query": "sketch", "uid": sketch}));
    assert_eq!(s["plane_link"], json!({"source": "O2", "target": "O1"}));
    assert_eq!(frame(&b.doc), before);
    assert_eq!(b.doc.feature(fillet).unwrap().component, base);
    all_ok(&b.doc);
    assert_eq!(
        query(&b.doc, json!({"query": "document"}))["display"]["isolated"],
        json!([{"body": b.body, "occurrence": "O2"}])
    );
    // The new occurrence moves: the sketch follows it.
    command(
        &mut b.doc,
        json!({"cmd": "ground_occurrence", "occurrence": "O2", "grounded": false}),
    );
    b.doc
        .set_occurrence_transform(
            OccurrenceUid(2),
            Transform::translation([0.0, 0.0, 10.0]),
            false,
        )
        .unwrap();
    let raised = frame(&b.doc);
    assert!(
        (raised["origin"][2].as_f64().unwrap() - before["origin"][2].as_f64().unwrap() - 10.0)
            .abs()
            < 1e-9
    );
}

#[test]
fn a_joint_on_the_roots_geometry_follows_it_into_the_component() {
    let mut doc = Document::new(MockKernel::default());
    let (_, base) = small_block(&mut doc, 0.0);
    let (pin, pin_occurrence) = doc
        .create_component(Some("Pin"), Transform::translation([100.0, 0.0, 0.0]), true)
        .unwrap();
    let (_, pin_body) = small_block(&mut doc, 0.0);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let face = |doc: &Document<MockKernel>, body: BodyUid, role: &str| {
        doc.body_shape(body)
            .unwrap()
            .faces
            .iter()
            .find(|f| f.role == role)
            .unwrap()
            .to_string()
    };
    // The pin's bottom on the root block's top.
    let (bottom, top) = (face(&doc, pin_body, "start"), face(&doc, base, "end"));
    let joint = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": "Pin:1", "geometry": {"body": pin_body, "face": bottom}},
               "b": {"occurrence": "", "geometry": {"body": base, "face": top}}}),
    );
    assert_eq!(joint["error"], Value::Null, "{joint}");
    let uid: FeatureUid = joint["uid"].as_str().unwrap().parse().unwrap();
    let pinned = doc.placement(pin_occurrence).unwrap();
    let before = placed(&doc);

    let moved = doc
        .move_to_components(ComponentUid::ROOT, &[], &[], false)
        .unwrap();
    // The joint stays in the root, its side b now seen through the new
    // occurrence; the pin stays where the joint put it.
    assert!(doc.feature(uid).unwrap().component.is_root());
    let joints = query(&doc, json!({"query": "joints"}));
    let row = joints["joints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["uid"] == uid.to_string())
        .unwrap()
        .clone();
    assert_eq!(row["b"]["path"], json!([moved[0].occurrence]), "{row}");
    assert_eq!(doc.placement(pin_occurrence), Some(pinned));
    assert_eq!(placed(&doc), before);
    assert_eq!(
        doc.feature(uid).map(|f| f.component),
        Some(ComponentUid::ROOT)
    );
    assert!(doc.features().filter(|f| f.component == pin).count() == 2);
    all_ok(&doc);
}

#[test]
fn a_new_component_operation_goes_along_and_its_component_stays() {
    let mut b = block();
    // A lid made as a component from the block's sketch.
    let lid = b
        .doc
        .add_feature(
            &extrude(b.sketch, &[&b.region], 5.0, "new_component", &[]),
            None,
        )
        .unwrap()
        .uid;
    let lid_component = b.doc.assembly().created_by(lid).unwrap();
    let before = placed(&b.doc);
    let moved = b
        .doc
        .move_to_components(ComponentUid::ROOT, &[], &[], false)
        .unwrap();
    assert_eq!(moved.len(), 1);
    // The sketch, the block and the lid's extrude go; the lid stays placed
    // in the root.
    assert_eq!(moved[0].features, vec![b.sketch, b.extrude, lid]);
    let lid_occurrence = b
        .doc
        .assembly()
        .occurrences_of(lid_component)
        .next()
        .unwrap();
    assert!(lid_occurrence.parent.is_root());
    assert_eq!(
        b.doc.body_component(BodyUid::new(lid, 0)),
        Some(lid_component)
    );
    assert_eq!(placed(&b.doc), before);
    all_ok(&b.doc);
}

#[test]
fn a_feature_that_ties_bodies_refuses_a_split_unless_they_stay_together() {
    let mut doc = Document::new(MockKernel::default());
    let (first, a) = small_block(&mut doc, 0.0);
    let (_, c) = small_block(&mut doc, 20.0);
    // A cut through all bodies works on both.
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_circle(sketch, [5.0, 5.0], &num(2.0))
        .unwrap()
        .region;
    let cut = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "cut", &[]), None)
        .unwrap()
        .uid;
    let (_, d) = small_block(&mut doc, 40.0);
    let depth = doc.undo_depth();

    // One component per body: the cut ties a and c.
    let plan = query(
        &doc,
        json!({"query": "move_to_component", "groups": [[a], [c], [d]]}),
    );
    let conflicts = plan["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{plan}");
    assert_eq!(conflicts[0]["feature"], json!(cut));
    assert_eq!(conflicts[0]["groups"], json!([0, 1]));
    let message = conflicts[0]["message"].as_str().unwrap();
    assert!(message.contains("different components"), "{message}");
    // Kept together: a and c in one component, d in its own.
    let together = plan["together"].as_array().unwrap();
    assert_eq!(together.len(), 2);
    assert_eq!(
        together[0]["bodies"],
        json!([{"uid": a, "name": "Body1"}, {"uid": c, "name": "Body2"}])
    );
    let error = rejected(
        &mut doc,
        json!({"cmd": "move_to_component", "groups": [[a], [c], [d]]}),
    );
    assert!(error.contains("different components"), "{error}");
    assert_eq!(doc.undo_depth(), depth);

    // Only a: the cut also works on c, which stays.
    let error = rejected(&mut doc, json!({"cmd": "move_to_component", "bodies": [a]}));
    assert!(error.contains("which stays in Root"), "{error}");
    // d alone is free of the cut.
    let moved = command(&mut doc, json!({"cmd": "move_to_component", "bodies": [d]}));
    assert_eq!(moved["components"][0]["name"], "Body3");
    assert_eq!(doc.component_bodies(ComponentUid::ROOT), vec![a, c]);
    doc.undo();

    let before = placed(&doc);
    let moved = command(
        &mut doc,
        json!({"cmd": "move_to_component", "groups": [[a], [c], [d]], "together": true}),
    );
    let made = moved["components"].as_array().unwrap();
    assert_eq!(made.len(), 2);
    assert_eq!(made[0]["bodies"], json!([a, c]));
    assert!(
        made[0]["features"]
            .as_array()
            .unwrap()
            .contains(&json!(cut))
    );
    assert!(
        made[0]["features"]
            .as_array()
            .unwrap()
            .contains(&json!(first))
    );
    assert_eq!(made[1]["bodies"], json!([d]));
    // Split components are not grounded; nothing moved.
    assert!(doc.assembly().occurrences.iter().all(|o| !o.grounded));
    assert_eq!(placed(&doc), before);
    assert!(doc.component_bodies(ComponentUid::ROOT).is_empty());
    all_ok(&doc);
    assert_eq!(doc.undo_depth(), depth + 1);
}

#[test]
fn a_sketch_on_a_face_of_a_body_that_stays_keeps_it_together() {
    let mut doc = Document::new(MockKernel::default());
    let (first, a) = small_block(&mut doc, 0.0);
    let region = doc.sketch_output(FeatureUid(1)).unwrap().regions[0]
        .key
        .clone();
    // A boss on a's top face, joined to a second block c.
    let (_, c) = small_block(&mut doc, 20.0);
    let top = format!("{first}:end({region})");
    let sketch = doc
        .add_sketch(SketchPlane::Face {
            face: Box::new(top.parse().unwrap()),
            body: Some(a),
        })
        .unwrap()
        .uid;
    let boss = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(2.0), &num(2.0))
        .unwrap()
        .region;
    doc.add_feature(&extrude(sketch, &[&boss], 3.0, "join", &[c]), None)
        .unwrap();
    let plan = doc
        .plan_move_to_components(
            ComponentUid::ROOT,
            &[MoveGroup {
                bodies: vec![c],
                features: Vec::new(),
            }],
        )
        .unwrap();
    assert!(!plan.conflicts.is_empty());
    assert!(
        plan.conflicts.iter().any(|c| c.staying == vec![a]),
        "{plan:?}"
    );
}

#[test]
fn the_history_of_a_component_from_bodies_moves_into_it() {
    let mut b = block();
    let made = b.doc.components_from_bodies(&[b.body]).unwrap();
    let (step, component) = (made[0].0.uid, made[0].1);
    // A chamfer in the component after it.
    b.doc.activate_component(component).unwrap();
    let fillet = b
        .doc
        .add_feature(
            &def(json!({"type": "fillet", "body": b.body,
                        "edges": [format!("E{{{0}:side(c1[c4,c2])|{0}:side(c2[c1,c3])}}", b.extrude)],
                        "radius": 1})),
            None,
        )
        .unwrap()
        .uid;
    let before = placed(&b.doc);
    let result = command(
        &mut b.doc,
        json!({"cmd": "move_history_into_component", "feature": step}),
    );
    assert_eq!(result["features"], json!([b.sketch, b.extrude]));
    assert!(b.doc.feature(step).is_none());
    assert_eq!(components(&b.doc), vec![component, component, component]);
    let def = b.doc.assembly().component(component).unwrap();
    assert_eq!(def.created_by, None);
    assert_eq!(def.name, "Body1");
    assert_eq!(b.doc.body_component(b.body), Some(component));
    assert_eq!(placed(&b.doc), before);
    assert_eq!(b.doc.status(fillet), Some(&FeatureStatus::Ok));
    all_ok(&b.doc);
    assert_eq!(b.doc.undo_label(), Some("Move History into Body1"));
    b.doc.undo();
    assert!(b.doc.feature(step).is_some());
    assert_eq!(
        b.doc.feature(b.extrude).unwrap().component,
        ComponentUid::ROOT
    );

    // Not a component from bodies.
    let error = rejected(
        &mut b.doc,
        json!({"cmd": "move_history_into_component", "feature": b.extrude}),
    );
    assert!(error.contains("is not a component from bodies"), "{error}");
}

#[test]
fn the_history_stays_when_it_also_made_a_body_that_stays() {
    let mut doc = Document::new(MockKernel::default());
    // One sketch, two rectangles, one extrude: two bodies of one feature.
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let r1 = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let r2 = doc
        .add_rectangle(sketch, [20.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&r1, &r2], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    let bodies = doc.component_bodies(ComponentUid::ROOT);
    assert_eq!(bodies.len(), 2);
    let made = doc.components_from_bodies(&[bodies[0]]).unwrap();
    let error = doc
        .move_history_into_component(made[0].0.uid)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(&format!("{} also work on", doc.feature(uid).unwrap().name)),
        "{error}"
    );
}

#[test]
fn hybrid_assemblies_can_be_allowed_per_design() {
    let mut b = block();
    let document = |doc: &Document<MockKernel>| query(doc, json!({"query": "document"}));
    assert!(document(&b.doc).get("hybrid_assembly").is_none());
    command(&mut b.doc, json!({"cmd": "set_hybrid_assembly"}));
    assert!(b.doc.hybrid_assembly());
    assert_eq!(document(&b.doc)["hybrid_assembly"], true);
    assert_eq!(b.doc.undo_label(), Some("Allow Hybrid Assembly"));
    let saved = b.doc.to_json();
    assert_eq!(
        serde_json::from_str::<Value>(&saved).unwrap()["hybrid_assembly"],
        true
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert!(loaded.hybrid_assembly());
    b.doc.undo();
    assert!(!b.doc.hybrid_assembly());
    assert!(
        serde_json::from_str::<Value>(&b.doc.to_json())
            .unwrap()
            .get("hybrid_assembly")
            .is_none()
    );
}

#[test]
fn feature_references_name_their_components() {
    let mut b = block();
    command(&mut b.doc, json!({"cmd": "move_to_component"}));
    let base = b.doc.feature(b.sketch).unwrap().component;
    let refs = query(
        &b.doc,
        json!({"query": "feature_references",
               "def": {"type": "extrude", "profiles": [{"sketch": b.sketch, "region": b.region}],
                       "extent": {"type": "distance", "distance": 5}, "operation": "new_component"}}),
    );
    assert_eq!(refs["features"], json!([b.sketch]));
    assert_eq!(refs["components"], json!([base]));
}

fn same_placement(actual: &[(BodyUid, BoundingBox)], expected: &[(BodyUid, BoundingBox)]) {
    assert_eq!(actual.len(), expected.len());
    for ((a, actual), (b, expected)) in actual.iter().zip(expected) {
        assert_eq!(a, b);
        for axis in 0..3 {
            assert!(
                (actual.min[axis] - expected.min[axis]).abs() < 1e-7,
                "{actual:?} != {expected:?}"
            );
            assert!(
                (actual.max[axis] - expected.max[axis]).abs() < 1e-7,
                "{actual:?} != {expected:?}"
            );
        }
    }
}

fn refused_unchanged(doc: &mut Document<MockKernel>, command: Value, reason: &str) {
    let before = doc.to_json();
    let depth = doc.undo_depth();
    let error = rejected(doc, command);
    assert!(error.contains(reason), "{error}");
    assert_eq!(doc.to_json(), before);
    assert_eq!(doc.undo_depth(), depth);
}

#[test]
fn explicit_existing_destination_preserves_nested_world_geometry_and_history() {
    let mut doc = Document::new(MockKernel::default());
    let quarter_turn =
        Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2).unwrap();
    let (parent, parent_o) = doc
        .add_component(
            Some("Assembly"),
            ComponentUid::ROOT,
            Transform::translation([40.0, -20.0, 5.0]).after(&quarter_turn),
        )
        .unwrap();
    let (source, source_o) = doc
        .add_component(
            Some("Source"),
            parent,
            Transform::translation([3.0, 7.0, 11.0]),
        )
        .unwrap();
    let (target, target_o) = doc
        .add_component(
            Some("Target"),
            ComponentUid::ROOT,
            Transform::translation([-30.0, 60.0, 10.0]),
        )
        .unwrap();
    doc.activate_component(target).unwrap();
    let (_, target_body) = small_block(&mut doc, 100.0);
    doc.activate_component(source).unwrap();
    let (extrude, body) = small_block(&mut doc, 2.0);
    let sketch = doc
        .features()
        .find(|f| f.component == source && f.def.type_name() == "sketch")
        .unwrap()
        .uid;
    let before_sketch = doc.feature(sketch).unwrap().def.clone();
    let old_frame = doc.sketch_output(sketch).unwrap().frame;
    let source_path = vec![parent_o, source_o];
    let old_world = doc.path_transform(&source_path).apply_frame(&old_frame);
    let before = placed(&doc);
    let saved_before = doc.to_json();
    let depth = doc.undo_depth();
    let count = doc.assembly().components.len();
    let destination = json!({"kind": "existing", "component": target, "occurrence": target_o});
    let plan = query(
        &doc,
        json!({"query": "move_to_component", "component": source,
        "features": [sketch], "source_occurrence": format!("{parent_o}/{source_o}"),
        "destination": destination}),
    );
    assert_eq!(plan["problems"], json!([]), "{plan}");
    let result = command(
        &mut doc,
        json!({"cmd": "move_to_component", "component": source,
        "features": [sketch], "source_occurrence": format!("{parent_o}/{source_o}"),
        "destination": destination}),
    );
    assert_eq!(result["components"][0]["created"], false);
    assert_eq!(result["components"][0]["component"], json!(target));
    assert_eq!(doc.assembly().components.len(), count);
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(doc.feature(extrude).unwrap().component, target);
    assert_eq!(doc.body_component(body), Some(target));
    assert_eq!(doc.body_component(target_body), Some(target));
    same_placement(&placed(&doc), &before);
    let world = doc
        .path_transform(&[target_o])
        .apply_frame(&doc.sketch_output(sketch).unwrap().frame);
    for (actual, expected) in world
        .origin
        .iter()
        .chain(&world.x_axis)
        .chain(&world.y_axis)
        .zip(
            old_world
                .origin
                .iter()
                .chain(&old_world.x_axis)
                .chain(&old_world.y_axis),
        )
    {
        assert!((actual - expected).abs() < 1e-9);
    }
    let mut unchanged = doc.feature(sketch).unwrap().def.clone();
    if let mitcad_model::features::FeatureDef::Sketch(s) = &mut unchanged {
        s.frame = None;
    }
    assert_eq!(unchanged, before_sketch); // constraints, parameters and entities kept
    all_ok(&doc);
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    same_placement(&placed(&loaded), &before);
    all_ok(&loaded);
    doc.undo();
    assert_eq!(doc.to_json(), saved_before);
    doc.redo();
    assert_eq!(doc.to_json(), saved);
    let mitcad_model::features::FeatureDef::Extrude(extrusion) =
        &loaded.feature(extrude).unwrap().def
    else {
        unreachable!()
    };
    let mitcad_model::features::Extent::Distance { distance, .. } = extrusion.extent else {
        unreachable!()
    };
    let parameter = loaded.parameters().get(distance).unwrap().name().to_owned();
    loaded.set_parameter(&parameter, 9.0).unwrap();
    all_ok(&loaded);
    let after_edit = placed(&loaded);
    let changed = after_edit.iter().find(|(b, _)| *b == body).unwrap().1;
    let old = before.iter().find(|(b, _)| *b == body).unwrap().1;
    assert!((changed.max[2] - old.max[2] - 4.0).abs() < 1e-7);
    let unchanged = after_edit.iter().find(|(b, _)| *b == target_body).unwrap();
    same_placement(
        &[*unchanged],
        &[*before.iter().find(|(b, _)| *b == target_body).unwrap()],
    );
}

#[test]
fn explicit_new_destination_uses_the_chosen_parent_and_one_undo_step() {
    let mut b = block();
    let (parent, occurrence) = b
        .doc
        .create_component(
            Some("Assembly"),
            Transform::translation([100.0, 20.0, -5.0]),
            false,
        )
        .unwrap();
    let before = placed(&b.doc);
    let saved = b.doc.to_json();
    let depth = b.doc.undo_depth();
    let result = command(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [b.sketch],
        "destination": {"kind": "new", "parent": parent, "parent_occurrence": occurrence, "name": "Moved"}}),
    );
    let made = &result["components"][0];
    assert_eq!(made["created"], true);
    assert_eq!(made["name"], "Moved");
    let component: ComponentUid = made["component"].as_str().unwrap().parse().unwrap();
    let new_o = b.doc.assembly().occurrences_of(component).next().unwrap();
    assert_eq!(new_o.parent, parent);
    assert_eq!(new_o.transform.translation, [-100.0, -20.0, 5.0]);
    assert_eq!(made["occurrence"], format!("{occurrence}/{}", new_o.uid));
    same_placement(&placed(&b.doc), &before);
    assert_eq!(b.doc.undo_depth(), depth + 1);
    b.doc.undo();
    assert_eq!(b.doc.to_json(), saved);
    b.doc.redo();
    same_placement(&placed(&b.doc), &before);
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "component": component,
        "destination": {"kind": "new", "parent": parent, "parent_occurrence": occurrence, "name": "Assembly"}}),
        "already named",
    );
}

#[test]
fn explicit_destination_query_lists_paths_and_refuses_wrong_or_shared_contexts() {
    let mut b = block();
    let (parent, p) = b
        .doc
        .create_component(Some("Parent"), Transform::IDENTITY, false)
        .unwrap();
    let (target, o) = b
        .doc
        .add_component(Some("Target"), parent, Transform::IDENTITY)
        .unwrap();
    let path = format!("{p}/{o}");
    let plan = query(
        &b.doc,
        json!({"query": "move_to_component", "features": [b.sketch]}),
    );
    let items = plan["destinations"].as_array().unwrap();
    let target_row = items.iter().find(|v| v["occurrence"] == path).unwrap();
    assert_eq!(target_row["path"], "Root/Parent:1/Target:1");
    assert_eq!(target_row["parent_occurrence"], json!(p));
    assert_eq!(target_row["eligible"], true);
    let root = items.iter().find(|v| v["component"] == "C0").unwrap();
    assert_eq!(root["eligible"], false);
    assert_eq!(root["can_create"], true);
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [b.sketch],
        "destination": {"kind": "existing", "component": "C0", "occurrence": ""}}),
        "already owns",
    );
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [b.sketch],
        "destination": {"kind": "existing", "component": target, "occurrence": o}}),
        "is not the placement",
    );
    b.doc
        .add_occurrence(
            parent,
            ComponentUid::ROOT,
            Transform::translation([200.0, 0.0, 0.0]),
        )
        .unwrap();
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [b.sketch],
        "destination": {"kind": "existing", "component": target, "occurrence": path}}),
        "exactly one occurrence",
    );
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [b.sketch],
        "destination": {"kind": "new", "parent": parent, "parent_occurrence": p, "name": "New"}}),
        "exactly one occurrence",
    );
    // A repeated source is refused even with an explicit selected path.
    b.doc.activate_component(target).unwrap();
    let (_, body) = small_block(&mut b.doc, 0.0);
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "component": target,
        "bodies": [body], "source_occurrence": path,
        "destination": {"kind": "existing", "component": "C0", "occurrence": ""}}),
        "exactly one occurrence",
    );
}

#[test]
fn an_unsupported_rebase_and_a_feature_created_destination_are_atomic_refusals() {
    let mut b = block();
    let top = format!("{}:end({})", b.extrude, b.region);
    let sketch = b
        .doc
        .add_sketch(SketchPlane::Face {
            face: Box::new(top.parse().unwrap()),
            body: Some(b.body),
        })
        .unwrap()
        .uid;
    let (target, occurrence) = b
        .doc
        .create_component(
            Some("Target"),
            Transform::translation([20.0, 0.0, 0.0]),
            false,
        )
        .unwrap();
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [sketch],
        "destination": {"kind": "existing", "component": target, "occurrence": occurrence}}),
        "cannot yet preserve its geometry",
    );
    let made = b.doc.components_from_bodies(&[b.body]).unwrap();
    let component = made[0].1;
    let occurrence = b
        .doc
        .assembly()
        .occurrences_of(component)
        .next()
        .unwrap()
        .uid;
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [b.sketch],
        "destination": {"kind": "existing", "component": component, "occurrence": occurrence}}),
        "created by a modeling feature",
    );
}

#[test]
fn explicit_existing_destination_keeps_external_support_links_and_isolation() {
    let mut b = block();
    let (_, observer) = b
        .doc
        .create_component(
            Some("Observer"),
            Transform::translation([0.0, 50.0, 0.0]),
            true,
        )
        .unwrap();
    mitcad_model::test_support::allow_attachments(&mut b.doc);
    let top = format!("{}:end({})", b.extrude, b.region);
    let sketch = command(
        &mut b.doc,
        json!({"cmd": "sketch.create",
        "plane": {"face": top, "body": b.body}, "occurrence": ""}),
    )["uid"]
        .clone();
    let frame = query(&b.doc, json!({"query": "sketch", "uid": sketch}))["frame"].clone();
    let (parent, p) = b
        .doc
        .add_component(
            Some("Parent"),
            ComponentUid::ROOT,
            Transform::translation([20.0, 30.0, 10.0]),
        )
        .unwrap();
    let (target, o) = b
        .doc
        .add_component(Some("Target"), parent, Transform::IDENTITY)
        .unwrap();
    command(
        &mut b.doc,
        json!({"cmd": "set_isolation", "items": [{"body": b.body}]}),
    );
    // Target takes the attachment that moves with the face (mitcad#220).
    mitcad_model::test_support::allow_attachments(&mut b.doc);
    command(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [b.sketch],
        "destination": {"kind": "existing", "component": target, "occurrence": format!("{p}/{o}")}}),
    );
    let after = query(&b.doc, json!({"query": "sketch", "uid": sketch}));
    assert_eq!(
        after["plane_link"],
        json!({"source": format!("{p}/{o}"), "target": observer})
    );
    assert_eq!(after["frame"], frame);
    assert_eq!(
        query(&b.doc, json!({"query": "document"}))["display"]["isolated"],
        json!([{"body": b.body, "occurrence": format!("{p}/{o}")}])
    );
    all_ok(&b.doc);
}

#[test]
fn explicit_destination_preserves_keep_together_consent() {
    let mut doc = Document::new(MockKernel::default());
    let (first, a) = small_block(&mut doc, 0.0);
    let (_, b) = small_block(&mut doc, 20.0);
    let region = doc.sketch_output(FeatureUid(1)).unwrap().regions[0]
        .key
        .clone();
    let cut = doc
        .add_feature(
            &extrude(FeatureUid(1), &[&region], 10.0, "cut", &[a, b]),
            None,
        )
        .unwrap()
        .uid;
    let (target, occurrence) = doc
        .create_component(Some("Target"), Transform::IDENTITY, false)
        .unwrap();
    let destination = json!({"kind": "existing", "component": target, "occurrence": occurrence});
    let plan = query(
        &doc,
        json!({"query": "move_to_component", "bodies": [a], "destination": destination}),
    );
    assert!(!plan["conflicts"].as_array().unwrap().is_empty());
    assert_eq!(plan["problems"], json!([]), "{plan}");
    refused_unchanged(
        &mut doc,
        json!({"cmd": "move_to_component", "bodies": [a],
        "destination": destination}),
        "keep those bodies together",
    );
    let result = command(
        &mut doc,
        json!({"cmd": "move_to_component", "bodies": [a],
        "destination": destination, "together": true}),
    );
    assert_eq!(result["components"][0]["bodies"], json!([a, b]));
    for f in [first, cut] {
        assert_eq!(doc.feature(f).unwrap().component, target);
    }
    all_ok(&doc);
}

#[test]
fn implicit_target_body_selection_is_not_silently_expanded_by_a_move() {
    let mut b = block();
    let (target, occurrence) = b
        .doc
        .create_component(Some("Target"), Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut b.doc, 0.0);
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, [0.0; 2], &num(2.0), &num(2.0))
        .unwrap()
        .region;
    b.doc
        .add_feature(&extrude(sketch, &[&region], 2.0, "cut", &[]), None)
        .unwrap();
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [b.sketch],
        "destination": {"kind": "existing", "component": target, "occurrence": occurrence}}),
        "reads all bodies",
    );
}

#[test]
fn origin_plane_frame_overrides_rebase_into_the_root_without_changing_sketch_coordinates() {
    for plane in ["xy", "xz", "yz"] {
        let mut doc = Document::new(MockKernel::default());
        let placement = Transform::translation([12.0, 20.0, 30.0])
            .after(&Transform::rotation([0.0; 3], [0.0, 1.0, 0.0], 0.4).unwrap());
        let (source, occurrence) = doc
            .create_component(Some("Source"), placement, true)
            .unwrap();
        let sketch = doc.add_feature(&def(json!({"type": "sketch", "plane": plane,
            "frame": {"origin": [3.0, 4.0, 5.0], "x_axis": [0.0, 1.0, 0.0], "y_axis": [-1.0, 0.0, 0.0]}})), None).unwrap().uid;
        let before = placement.apply_frame(&doc.sketch_output(sketch).unwrap().frame);
        command(
            &mut doc,
            json!({"cmd": "move_to_component", "component": source,
            "features": [sketch], "source_occurrence": occurrence,
            "destination": {"kind": "existing", "component": "C0", "occurrence": ""}}),
        );
        let after = doc.sketch_output(sketch).unwrap().frame;
        for point in [[0.0, 0.0], [5.0, -7.0], [17.0, 9.0]] {
            for (a, b) in after.point(point).iter().zip(before.point(point)) {
                assert!((a - b).abs() < 1e-9, "{plane}: {after:?} != {before:?}");
            }
        }
        all_ok(&doc);
    }
}

#[test]
fn timeline_positioned_destinations_are_refused_even_at_the_original_placement() {
    for distance in [0, 10] {
        let mut b = block();
        let (target, occurrence) = b
            .doc
            .create_component(Some("Target"), Transform::IDENTITY, false)
            .unwrap();
        command(
            &mut b.doc,
            json!({"cmd": "add_feature", "def": {
                "type": "move_occurrence", "occurrences": [occurrence],
                "transform": {"type": "translate_xyz", "x": distance, "y": 0, "z": 0}
            }}),
        );
        refused_unchanged(
            &mut b.doc,
            json!({"cmd": "move_to_component", "features": [b.sketch],
            "destination": {"kind": "existing", "component": target, "occurrence": occurrence}}),
            "placed by a timeline feature",
        );
    }
}

#[test]
fn readonly_destinations_and_cancelled_creation_leave_no_component_or_undo_step() {
    let dir = mitcad_model::testing::unique_temp_dir("move-destination");
    let part = block();
    std::fs::write(dir.join("part.mitcad"), part.doc.to_json()).unwrap();
    let mut b = block();
    let (linked, occurrence) = b
        .doc
        .insert_component(
            "part.mitcad",
            &mitcad_model::InsertOptions {
                base: Some(dir.clone()),
                ..mitcad_model::InsertOptions::default()
            },
        )
        .unwrap();
    let destination = json!({"kind": "existing", "component": linked, "occurrence": occurrence});
    let plan = query(
        &b.doc,
        json!({"query": "move_to_component", "features": [b.sketch],
        "destination": destination}),
    );
    assert!(!plan["problems"].as_array().unwrap().is_empty());
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [b.sketch],
        "destination": destination}),
        "linked from",
    );
    let before = placed(&b.doc);
    let monitor = std::sync::Arc::new(mitcad_model::RecomputeMonitor::new());
    monitor.cancel();
    b.doc.set_monitor(Some(monitor));
    refused_unchanged(
        &mut b.doc,
        json!({"cmd": "move_to_component", "features": [b.sketch],
        "destination": {"kind": "new", "parent": "C0", "parent_occurrence": "", "name": "Cancelled"}}),
        "computation was canceled",
    );
    same_placement(&placed(&b.doc), &before);
    b.doc.set_monitor(None);
    mitcad_model::testing::remove_temp(&dir);
}

/// The cut scope every extrusion made in the UI stores (mitcad#859): its
/// own component as the context, automatic targets; a new body's waits
/// dormant until the extrusion cuts.
fn ui_scope(context: Value, dormant: bool) -> Value {
    let mut scope = json!({"context": context,
        "targets": {"type": "automatic", "excluded": [], "excluded_bodies": []}});
    if dormant {
        scope["initialize_on_cut"] = json!(true);
    }
    scope
}

fn root_context() -> Value {
    json!({"component": "C0", "occurrence": ""})
}

/// A sketch with a rectangle at `corner` and its extrusion 5 mm with
/// `scope`, in the active component, as the UI makes them.
fn ui_extrude(
    doc: &mut Document<MockKernel>,
    corner: [f64; 2],
    size: [f64; 2],
    operation: &str,
    scope: Value,
) -> FeatureUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, corner, &num(size[0]), &num(size[1]))
        .unwrap()
        .region;
    doc.add_feature(
        &def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                    "extent": {"type": "distance", "distance": 5},
                    "operation": operation, "cut_scope": scope}),
        ),
        None,
    )
    .unwrap()
    .uid
}

fn scope_of(doc: &Document<MockKernel>, uid: FeatureUid) -> Value {
    let FeatureDef::Extrude(extrusion) = &doc.feature(uid).unwrap().def else {
        unreachable!()
    };
    serde_json::to_value(extrusion.cut_scope.as_ref().unwrap()).unwrap()
}

fn volumes(doc: &Document<MockKernel>, bodies: &[BodyUid]) -> Vec<f64> {
    bodies
        .iter()
        .map(|b| {
            doc.kernel()
                .mass_properties(doc.body_shape(*b).unwrap())
                .unwrap()
                .volume
        })
        .collect()
}

fn same_volumes(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (a - e).abs() <= 1e-6 * e.abs(),
            "{actual:?} != {expected:?}"
        );
    }
}

fn is_cut(doc: &Document<MockKernel>, body: BodyUid) -> bool {
    doc.body_shape(body).unwrap().history.starts_with("cut(")
}

/// The extrusion's definition edits back with another distance: its
/// moved scope still checks.
fn editable(doc: &mut Document<MockKernel>, uid: FeatureUid) {
    let mut def = query(doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    def["extent"]["distance"] = json!(4);
    let edit = command(doc, json!({"cmd": "edit_feature", "uid": uid, "def": def}));
    assert_eq!(edit["error"], Value::Null, "{edit}");
    all_ok(doc);
    doc.undo();
}

/// The root's two sketches and extrusions as the UI makes them: Body1 and
/// a pocket cut into it.
fn ui_part(doc: &mut Document<MockKernel>) -> (FeatureUid, FeatureUid, BodyUid) {
    let boss = ui_extrude(
        doc,
        [0.0, 0.0],
        [10.0, 10.0],
        "new_body",
        ui_scope(root_context(), true),
    );
    let cut = ui_extrude(
        doc,
        [2.0, 2.0],
        [4.0, 4.0],
        "cut",
        ui_scope(root_context(), false),
    );
    let body = BodyUid::new(boss, 0);
    all_ok(doc);
    assert!(is_cut(doc, body));
    (boss, cut, body)
}

fn reopens(doc: &Document<MockKernel>, world: &[(BodyUid, BoundingBox)]) {
    let mut loaded = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    loaded.recompute();
    all_ok(&loaded);
    same_placement(&placed(&loaded), world);
}

#[test]
fn ui_extrusions_move_into_an_existing_component_with_their_cut_scopes() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    let (component, occurrence) = doc
        .create_component(
            Some("Component1"),
            Transform::translation([30.0, -10.0, 0.0]),
            false,
        )
        .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (boss, cut, body) = ui_part(&mut doc);
    let before = volumes(&doc, &[body]);
    let world = placed(&doc);
    let destination = json!({"kind": "existing", "component": component, "occurrence": occurrence});

    // The dialog's preview reports nothing in the way.
    let plan = query(
        &doc,
        json!({"query": "move_to_component", "bodies": [body], "destination": destination}),
    );
    assert_eq!(plan["problems"], json!([]), "{plan}");
    assert_eq!(
        plan["groups"][0]["features"].as_array().unwrap().len(),
        4,
        "{plan}"
    );
    let result = command(
        &mut doc,
        json!({"cmd": "move_to_component", "bodies": [body], "destination": destination}),
    );
    assert_eq!(result["components"][0]["component"], json!(component));
    let context = json!({"component": component, "occurrence": occurrence});
    for uid in [boss, cut] {
        assert_eq!(doc.feature(uid).unwrap().component, component);
        assert_eq!(scope_of(&doc, uid)["context"], context);
    }
    // The new body's scope stays dormant; the cut's targets are automatic.
    assert_eq!(scope_of(&doc, boss)["initialize_on_cut"], true);
    assert_eq!(
        scope_of(&doc, cut)["targets"],
        json!({"type": "automatic", "excluded": [], "excluded_bodies": []})
    );
    all_ok(&doc);
    assert!(is_cut(&doc, body));
    same_volumes(&volumes(&doc, &[body]), &before);
    same_placement(&placed(&doc), &world);
    editable(&mut doc, cut);
    editable(&mut doc, boss);
    reopens(&doc, &world);
}

#[test]
fn ui_extrusions_move_into_a_new_component_with_their_cut_scopes() {
    let explicit = json!({"kind": "new", "parent": "C0", "parent_occurrence": "", "name": "Part"});
    for destination in [None, Some(explicit)] {
        let mut doc = Document::new(MockKernel::default());
        doc.kernel().measures.set(true);
        let (boss, cut, body) = ui_part(&mut doc);
        let before = volumes(&doc, &[body]);
        let world = placed(&doc);
        let mut selection = json!({"query": "move_to_component"});
        if let Some(destination) = &destination {
            selection["destination"] = destination.clone();
        }
        let plan = query(&doc, selection.clone());
        assert_eq!(plan["problems"], json!([]), "{plan}");
        selection.as_object_mut().unwrap().remove("query");
        selection["cmd"] = json!("move_to_component");
        let result = command(&mut doc, selection);
        let made = &result["components"][0];
        let component: ComponentUid = made["component"].as_str().unwrap().parse().unwrap();
        let context = json!({"component": component, "occurrence": made["occurrence"]});
        for uid in [boss, cut] {
            assert_eq!(doc.feature(uid).unwrap().component, component);
            assert_eq!(scope_of(&doc, uid)["context"], context, "{destination:?}");
        }
        assert!(doc.component_bodies(ComponentUid::ROOT).is_empty());
        all_ok(&doc);
        assert!(is_cut(&doc, body));
        same_volumes(&volumes(&doc, &[body]), &before);
        same_placement(&placed(&doc), &world);
        editable(&mut doc, cut);
        editable(&mut doc, boss);
        reopens(&doc, &world);
        // Undo takes the scopes back with the features.
        doc.undo();
        for uid in [boss, cut] {
            assert_eq!(scope_of(&doc, uid)["context"], root_context());
        }
        all_ok(&doc);
    }
}

#[test]
fn a_cut_keeps_its_explicit_body_target_that_moves_with_it() {
    // Into a new component (everything), and into an existing one.
    for existing in [false, true] {
        let mut doc = Document::new(MockKernel::default());
        doc.kernel().measures.set(true);
        let target = existing.then(|| {
            let made = doc
                .create_component(
                    Some("Component1"),
                    Transform::translation([0.0, 40.0, 0.0]),
                    false,
                )
                .unwrap();
            doc.activate_component(ComponentUid::ROOT).unwrap();
            made
        });
        let first = ui_extrude(
            &mut doc,
            [0.0, 0.0],
            [10.0, 10.0],
            "new_body",
            ui_scope(root_context(), true),
        );
        let second = ui_extrude(
            &mut doc,
            [20.0, 0.0],
            [10.0, 10.0],
            "new_body",
            ui_scope(root_context(), true),
        );
        let (a, b) = (BodyUid::new(first, 0), BodyUid::new(second, 0));
        // The cut reaches both bodies; the second is left out by name.
        let cut = ui_extrude(
            &mut doc,
            [5.0, 2.0],
            [20.0, 4.0],
            "cut",
            json!({"context": root_context(),
                   "targets": {"type": "explicit", "included": [root_context()],
                               "excluded_bodies": [{"component": "C0", "occurrence": "", "body": b}]}}),
        );
        all_ok(&doc);
        assert!(is_cut(&doc, a));
        assert!(!is_cut(&doc, b));
        let before = volumes(&doc, &[a, b]);
        let world = placed(&doc);
        let mut selection = json!({"cmd": "move_to_component"});
        if let Some((component, occurrence)) = target {
            selection["bodies"] = json!([a, b]);
            selection["destination"] =
                json!({"kind": "existing", "component": component, "occurrence": occurrence});
        }
        let result = command(&mut doc, selection);
        let made = &result["components"][0];
        let component: ComponentUid = made["component"].as_str().unwrap().parse().unwrap();
        if let Some((existing, _)) = target {
            assert_eq!(component, existing);
        }
        let place = json!({"component": component, "occurrence": made["occurrence"]});
        assert_eq!(doc.feature(cut).unwrap().component, component);
        assert_eq!(doc.body_component(b), Some(component));
        let mut body_target = place.clone();
        body_target["body"] = json!(b);
        assert_eq!(
            scope_of(&doc, cut),
            json!({"context": place,
                   "targets": {"type": "explicit", "included": [place],
                               "excluded_bodies": [body_target]}})
        );
        all_ok(&doc);
        assert!(is_cut(&doc, a));
        assert!(!is_cut(&doc, b));
        same_volumes(&volumes(&doc, &[a, b]), &before);
        same_placement(&placed(&doc), &world);
        editable(&mut doc, cut);
        reopens(&doc, &world);
    }
}

#[test]
fn another_components_cut_follows_a_body_it_leaves_out() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    let first = ui_extrude(
        &mut doc,
        [0.0, 0.0],
        [10.0, 10.0],
        "new_body",
        ui_scope(root_context(), true),
    );
    let root_body = BodyUid::new(first, 0);
    // Other:1 at the same place, with a body of its own and a cut through
    // both that leaves the root's body out.
    let (other, occurrence) = doc
        .create_component(Some("Other"), Transform::IDENTITY, true)
        .unwrap();
    let own = json!({"component": other, "occurrence": occurrence});
    let boss = ui_extrude(
        &mut doc,
        [0.0, 0.0],
        [10.0, 10.0],
        "new_body",
        ui_scope(own.clone(), true),
    );
    let other_body = BodyUid::new(boss, 0);
    let cut = ui_extrude(
        &mut doc,
        [2.0, 2.0],
        [4.0, 4.0],
        "cut",
        json!({"context": own,
               "targets": {"type": "automatic", "excluded": [],
                           "excluded_bodies": [{"component": "C0", "occurrence": "", "body": root_body}]}}),
    );
    all_ok(&doc);
    assert!(is_cut(&doc, other_body));
    assert!(!is_cut(&doc, root_body));
    let before = volumes(&doc, &[root_body, other_body]);
    let world = placed(&doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();

    let result = command(&mut doc, json!({"cmd": "move_to_component"}));
    let made = &result["components"][0];
    // The cut stays in Other; the body it leaves out is named where it went.
    assert_eq!(doc.feature(cut).unwrap().component, other);
    assert_eq!(
        scope_of(&doc, cut)["targets"]["excluded_bodies"],
        json!([{"component": made["component"], "occurrence": made["occurrence"], "body": root_body}])
    );
    assert_eq!(scope_of(&doc, cut)["context"], own);
    all_ok(&doc);
    assert!(is_cut(&doc, other_body));
    assert!(!is_cut(&doc, root_body));
    same_volumes(&volumes(&doc, &[root_body, other_body]), &before);
    same_placement(&placed(&doc), &world);
    editable(&mut doc, cut);
}
