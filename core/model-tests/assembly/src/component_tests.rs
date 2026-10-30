// SPDX-License-Identifier: MIT
//! Components and occurrences (F6) with the mock kernel: shared
//! definitions, placements, nesting, the active component, new-component
//! operations, timeline moves, copies, external components and the
//! design file.

use serde_json::{Value, json};

use mitcad_model::assembly::{from_rows, matrix_rows};
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::test_support::{block, def, extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, FeatureStatus, InsertOptions};
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

fn shift(x: f64, y: f64, z: f64) -> Transform {
    Transform::translation([x, y, z])
}

/// A 10 x 10 x 5 block (Sketch and Extrude) in the active component.
fn small_block(doc: &mut Document<MockKernel>) -> (FeatureUid, BodyUid) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (uid, BodyUid::new(uid, 0))
}

/// The bounding boxes of the visible instances in the design, by path.
fn world_boxes(doc: &Document<MockKernel>) -> Vec<(String, BodyUid, BoundingBox)> {
    doc.instances()
        .into_iter()
        .filter(|i| i.visible)
        .map(|i| {
            let shape = doc.body_shape(i.body).unwrap();
            let placed = doc
                .kernel()
                .transform_shape(shape, &i.transform, None)
                .unwrap();
            (i.path_name, i.body, placed.bounds.unwrap())
        })
        .collect()
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

#[test]
fn occurrences_share_their_component() {
    let mut doc = Document::new(MockKernel::default());
    let (component, first) = doc
        .create_component(Some("Plate"), shift(100.0, 0.0, 0.0), true)
        .unwrap();
    assert_eq!(doc.active_component(), component);
    let (extrude_uid, body) = small_block(&mut doc);
    assert_eq!(doc.feature(extrude_uid).unwrap().component, component);
    assert_eq!(doc.body_component(body), Some(component));
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let second = doc
        .copy_occurrence(first, Some(shift(0.0, 50.0, 0.0)))
        .unwrap();
    assert_eq!(doc.assembly().occurrence_name(second), "Plate:2");

    let boxes = world_boxes(&doc);
    assert_eq!(boxes.len(), 2);
    assert_eq!(boxes[0].0, "Plate:1");
    assert!(near(boxes[0].2.min, [100.0, 0.0, 0.0]) && near(boxes[0].2.max, [110.0, 10.0, 5.0]));
    assert!(near(boxes[1].2.min, [0.0, 50.0, 0.0]) && near(boxes[1].2.max, [10.0, 60.0, 5.0]));

    // One definition: a change of the component shows in both.
    let d = doc
        .parameters()
        .iter()
        .find(|p| p.comment() == "Extrude1 distance")
        .unwrap()
        .name()
        .to_owned();
    doc.set_parameter(&d, 8.0).unwrap();
    let boxes = world_boxes(&doc);
    assert!(
        boxes.iter().all(|b| (b.2.max[2] - 8.0).abs() < 1e-9),
        "{boxes:?}"
    );
    // The body is computed once, in the component's coordinates.
    assert_eq!(doc.bodies().len(), 1);
    let bounds = doc.body_shape(body).unwrap().bounds.unwrap();
    assert!(near(bounds.min, [0.0; 3]));
}

#[test]
fn nested_occurrences_compose_their_placements() {
    let mut doc = Document::new(MockKernel::default());
    let (arm, arm1) = doc
        .create_component(Some("Arm"), shift(100.0, 0.0, 0.0), true)
        .unwrap();
    small_block(&mut doc);
    // A pin placed in the arm, turned a quarter about Z.
    let quarter =
        Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2).unwrap();
    let (pin, _) = doc
        .create_component(Some("Pin"), shift(0.0, 0.0, 5.0).after(&quarter), true)
        .unwrap();
    small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let arm2 = doc
        .copy_occurrence(arm1, Some(shift(0.0, 200.0, 0.0)))
        .unwrap();
    assert_eq!(doc.assembly().paths_to(pin).len(), 2);

    let boxes = world_boxes(&doc);
    let names: Vec<&str> = boxes.iter().map(|b| b.0.as_str()).collect();
    assert_eq!(names, ["Arm:1", "Arm:1/Pin:1", "Arm:2", "Arm:2/Pin:1"]);
    // The pin's 10 x 10 block turned to -10..0 in x, raised 5, then moved
    // with each arm.
    assert!(
        near(boxes[1].2.min, [90.0, 0.0, 5.0]) && near(boxes[1].2.max, [100.0, 10.0, 10.0]),
        "{boxes:?}"
    );
    assert!(near(boxes[3].2.min, [-10.0, 200.0, 5.0]), "{boxes:?}");
    assert_eq!(
        doc.assembly().find_path("Arm:2/Pin:1"),
        Some(vec![arm2, OccurrenceUid(2)])
    );

    // Copies go into the active component, which must not be in them.
    doc.activate_component(pin).unwrap();
    let error = doc.copy_occurrence(arm1, None).unwrap_err().to_string();
    assert!(error.contains("Arm cannot be placed in Pin"), "{error}");
    doc.activate_component(arm).unwrap();
    assert!(doc.copy_occurrence(arm1, None).is_err());
    let pin1 = doc.assembly().occurrences_of(pin).next().unwrap().uid;
    let pin2 = doc.copy_occurrence(pin1, None).unwrap();
    assert_eq!(doc.assembly().occurrence(pin2).unwrap().parent, arm);
    assert_eq!(doc.assembly().paths_to(pin).len(), 4);
}

#[test]
fn features_go_into_the_active_component() {
    let mut doc = Document::new(MockKernel::default());
    let (_, root_body) = small_block(&mut doc);
    let (part, _) = doc
        .create_component(None, Transform::IDENTITY, true)
        .unwrap();
    let (_, part_body) = small_block(&mut doc);
    assert_eq!(doc.body_component(root_body), Some(ComponentUid::ROOT));
    assert_eq!(doc.body_component(part_body), Some(part));
    // A feature works on its own component's bodies.
    let fillet = def(json!({"type": "fillet", "body": root_body,
        "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 1}));
    let uid = doc.add_feature(&fillet, None).unwrap().uid;
    let status = doc.status(uid).unwrap().clone();
    assert!(
        matches!(&status, FeatureStatus::Failed(m) if m.contains("belongs to Root")),
        "{status:?}"
    );
    doc.undo();
    // With the component given, it works there.
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "component": "C0", "def": fillet_json(root_body)}),
    );
    assert_eq!(added["error"], Value::Null, "{added}");
    // A sketch of one component cannot be used by another's feature.
    let sketch = doc.features().find(|f| f.name == "Sketch2").unwrap().uid;
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let error = doc
        .add_feature(
            &def(json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
                        "extent": {"type": "distance", "distance": 3}, "operation": "new_body"})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("Sketch2 is in Component1"), "{error}");
    // The timeline names the component of the features not in the root.
    let timeline = query(&doc, json!({"query": "timeline"}));
    assert_eq!(timeline["features"][3]["component"], "C1");
    assert_eq!(timeline["features"][0].get("component"), None);
}

fn fillet_json(body: BodyUid) -> Value {
    json!({"type": "fillet", "body": body,
           "edges": [format!("E{{{0}:side(c1[c4,c2])|{0}:side(c2[c1,c3])}}", body.feature)],
           "radius": 1})
}

#[test]
fn new_component_operations_make_and_remove_components() {
    let mut b = block();
    let new = extrude(b.sketch, &[&b.region], 5.0, "new_component", &[]);
    let uid = b.doc.add_feature(&new, None).unwrap().uid;
    let made = b.doc.assembly().created_by(uid).unwrap();
    assert_eq!(b.doc.body_component(BodyUid::new(uid, 0)), Some(made));
    assert_eq!(b.doc.assembly().occurrences_of(made).count(), 1);
    // A fillet of the new body belongs to its component.
    b.doc.activate_component(made).unwrap();
    let fillet = b
        .doc
        .add_feature(&def(fillet_json(BodyUid::new(uid, 0))), None)
        .unwrap()
        .uid;
    assert_eq!(b.doc.status(fillet), Some(&FeatureStatus::Ok));
    // Another operation would leave the fillet's component without its
    // maker: refused.
    let error = b
        .doc
        .edit_feature(uid, &extrude(b.sketch, &[&b.region], 5.0, "new_body", &[]))
        .unwrap_err()
        .to_string();
    assert!(error.contains("has features or occurrences"), "{error}");
    b.doc.delete_feature(fillet, false).unwrap();
    b.doc
        .edit_feature(uid, &extrude(b.sketch, &[&b.region], 5.0, "new_body", &[]))
        .unwrap();
    assert!(b.doc.assembly().components.is_empty());
    assert_eq!(b.doc.active_component(), ComponentUid::ROOT);
    assert_eq!(
        b.doc.body_component(BodyUid::new(uid, 0)),
        Some(ComponentUid::ROOT)
    );
    // And back; deleting the feature removes its component.
    b.doc.edit_feature(uid, &new).unwrap();
    assert_eq!(b.doc.assembly().components.len(), 1);
    b.doc.delete_feature(uid, false).unwrap();
    assert!(b.doc.assembly().is_empty());
    // Undo brings it back.
    b.doc.undo();
    assert_eq!(b.doc.assembly().components.len(), 1);

    // Combine into a new component moves the target.
    let tool = b
        .doc
        .add_feature(&extrude(b.sketch, &[&b.region], 3.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    let combine = b
        .doc
        .add_feature(
            &def(
                json!({"type": "combine", "target": b.body, "tools": [BodyUid::new(tool, 0)],
                        "operation": "join", "new_component": true}),
            ),
            None,
        )
        .unwrap()
        .uid;
    let made = b.doc.assembly().created_by(combine).unwrap();
    assert_eq!(b.doc.body_component(b.body), Some(made));
}

#[test]
fn components_from_bodies_take_the_bodies() {
    let mut b = block();
    let made = b.doc.components_from_bodies(&[b.body]).unwrap();
    let (added, component) = &made[0];
    assert_eq!(b.doc.assembly().name(*component), "Body1");
    assert_eq!(b.doc.body_component(b.body), Some(*component));
    assert_eq!(
        b.doc.feature(added.uid).unwrap().def.type_name(),
        "component_from_bodies"
    );
    // Features of the root no longer reach it; features of the new
    // component do.
    let uid = b
        .doc
        .add_feature(&def(fillet_json(b.body)), None)
        .unwrap()
        .uid;
    assert!(b.doc.status(uid).unwrap().error().is_some());
    b.doc.undo();
    b.doc.activate_component(*component).unwrap();
    let uid = b
        .doc
        .add_feature(&def(fillet_json(b.body)), None)
        .unwrap()
        .uid;
    assert_eq!(b.doc.status(uid), Some(&FeatureStatus::Ok));
    // Deleting its occurrence deletes the component and gives the body
    // back (the feature that made the component goes too).
    let occurrence = b.doc.assembly().occurrences[0].uid;
    let deleted = b.doc.delete_occurrence(occurrence).unwrap();
    assert_eq!(deleted, vec![added.uid, uid]);
    assert_eq!(b.doc.body_component(b.body), Some(ComponentUid::ROOT));
}

#[test]
fn a_component_from_bodies_can_fill_an_existing_empty_component() {
    let mut b = block();
    let (base, occurrence) = b
        .doc
        .create_component(Some("Base"), shift(0.0, 0.0, 5.0), false)
        .unwrap();
    let made = b.doc.components_from_bodies(&[b.body]).unwrap();
    let (added, component) = &made[0];
    b.doc.move_made_component(added.uid, base).unwrap();
    // The body is the existing component's, placed by its occurrence; the
    // component the feature made is gone.
    let a = b.doc.assembly();
    assert!(a.component(*component).is_none());
    assert_eq!(a.created_by(added.uid), Some(base));
    assert_eq!(a.occurrences.len(), 1);
    assert_eq!(a.occurrences[0].uid, occurrence);
    assert_eq!(b.doc.body_component(b.body), Some(base));
    // Its features reach the body.
    b.doc.activate_component(base).unwrap();
    let uid = b
        .doc
        .add_feature(&def(fillet_json(b.body)), None)
        .unwrap()
        .uid;
    assert_eq!(b.doc.status(uid), Some(&FeatureStatus::Ok));
    // Only into an empty component: one with a feature is refused, as is a
    // feature that made none.
    let (other, _) = b
        .doc
        .create_component(Some("Other"), Transform::IDENTITY, false)
        .unwrap();
    let error = b
        .doc
        .move_made_component(added.uid, ComponentUid::ROOT)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not an empty component"), "{error}");
    let error = b
        .doc
        .move_made_component(uid, other)
        .unwrap_err()
        .to_string();
    assert!(error.contains("made no component"), "{error}");
    // Undo puts the made component back.
    b.doc.undo();
    b.doc.undo();
    b.doc.undo();
    b.doc.undo();
    assert_eq!(b.doc.body_component(b.body), Some(*component));
}

#[test]
fn timeline_moves_and_captured_positions_place_occurrences() {
    let mut doc = Document::new(MockKernel::default());
    let (_, o1) = doc
        .create_component(None, Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let moved = doc
        .add_feature(
            &def(json!({"type": "move_occurrence", "occurrences": [o1],
                        "transform": {"type": "translate_xyz", "x": 10, "y": 0, "z": 0}})),
            None,
        )
        .unwrap();
    assert_eq!(moved.name, "Move1");
    assert!(near(
        doc.placement(o1).unwrap().translation,
        [10.0, 0.0, 0.0]
    ));
    // Its own placement can no longer be set; a captured one can.
    let error = doc
        .set_occurrence_transform(o1, shift(0.0, 5.0, 0.0), false)
        .unwrap_err()
        .to_string();
    assert!(error.contains("placed by Move1"), "{error}");
    let captured = doc
        .set_occurrence_transform(o1, shift(0.0, 5.0, 0.0), true)
        .unwrap()
        .unwrap();
    assert_eq!(
        doc.feature(captured.uid).unwrap().def.type_name(),
        "capture_position"
    );
    assert!(near(
        doc.placement(o1).unwrap().translation,
        [0.0, 5.0, 0.0]
    ));
    // The marker before the capture: the move's placement.
    doc.set_marker(doc.marker() - 1).unwrap();
    assert!(near(
        doc.placement(o1).unwrap().translation,
        [10.0, 0.0, 0.0]
    ));
    doc.set_marker(doc.features().count()).unwrap();
    // The move's distance is a parameter.
    let d = doc
        .parameters()
        .iter()
        .find(|p| p.comment() == "Move1 x")
        .unwrap()
        .name()
        .to_owned();
    doc.set_marker(doc.marker() - 1).unwrap();
    doc.set_parameter(&d, 30.0).unwrap();
    assert!(near(
        doc.placement(o1).unwrap().translation,
        [30.0, 0.0, 0.0]
    ));

    // A grounded occurrence does not move.
    doc.set_occurrence_grounded(o1, true).unwrap();
    let status = doc.status(moved.uid).unwrap().clone();
    assert!(
        matches!(&status, FeatureStatus::Failed(m) if m.contains("grounded")),
        "{status:?}"
    );
    assert!(near(doc.placement(o1).unwrap().translation, [0.0; 3]));
    assert!(
        doc.set_occurrence_transform(o1, shift(1.0, 0.0, 0.0), true)
            .is_err()
    );
    doc.undo();
    assert_eq!(doc.status(moved.uid), Some(&FeatureStatus::Ok));
}

#[test]
fn ground_visibility_names_and_activation_are_commands() {
    let mut doc = Document::new(MockKernel::default());
    let made = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket",
                                        "transform": {"translation": [5, 0, 0]}}),
    );
    assert_eq!(made["component"], "C1");
    assert_eq!(made["occurrence"], "O1");
    small_block(&mut doc);
    for c in [
        json!({"cmd": "ground_occurrence", "occurrence": "Bracket:1"}),
        json!({"cmd": "set_occurrence_visible", "occurrence": "O1", "visible": false}),
        json!({"cmd": "rename_component", "component": "Bracket", "name": "Holder"}),
        json!({"cmd": "activate_component", "component": "C0"}),
    ] {
        command(&mut doc, c);
    }
    let components = query(&doc, json!({"query": "components"}));
    assert_eq!(components["active"], "C0");
    let o = &components["occurrences"][0];
    assert_eq!(o["name"], "Holder:1");
    assert_eq!(
        (o["grounded"].clone(), o["visible"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(o["world"][0], json!([1.0, 0.0, 0.0, 5.0]));
    assert_eq!(components["components"][1]["features"], json!(["F1", "F2"]));
    // Hidden: no visible instances.
    assert_eq!(query(&doc, json!({"query": "instances"})), json!([]));
    let all = query(&doc, json!({"query": "instances", "hidden": true}));
    assert_eq!(all[0]["occurrence"], "Holder:1");
    assert_eq!(all[0]["transform"][0], json!([1.0, 0.0, 0.0, 5.0]));
    // Undo steps: activation, rename, visibility, ground.
    for label in [
        "Activate Root",
        "Rename Bracket to Holder",
        "Hide Bracket:1",
        "Ground Bracket:1",
    ] {
        let undone = doc.undo().unwrap();
        assert!(
            undone.starts_with(&label[..label.find(' ').unwrap()]),
            "{undone} / {label}"
        );
    }
    // Names must be unique.
    let error = doc
        .rename_component(ComponentUid::ROOT, "Bracket")
        .unwrap_err();
    assert!(error.to_string().contains("already named"), "{error}");
}

#[test]
fn paste_new_copies_the_component_with_its_history() {
    let mut doc = Document::new(MockKernel::default());
    let (component, o1) = doc
        .create_component(Some("Part"), Transform::IDENTITY, true)
        .unwrap();
    let (_, body) = small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (copy, o2) = doc.paste_new(o1, Some(shift(50.0, 0.0, 0.0))).unwrap();
    assert_ne!(copy, component);
    assert_eq!(doc.assembly().name(copy), "Part (1)");
    let copied: Vec<_> = doc.features().filter(|f| f.component == copy).collect();
    assert_eq!(copied.len(), 2);
    assert_eq!(copied[0].name, "Sketch2");
    // The copy's extrude uses the copied sketch and its own parameters.
    let extrude_copy = copied[1].uid;
    let copy_body = BodyUid::new(extrude_copy, 0);
    assert_eq!(doc.body_component(copy_body), Some(copy));
    let original = doc
        .parameters()
        .iter()
        .find(|p| p.comment() == "Extrude1 distance")
        .unwrap()
        .name()
        .to_owned();
    doc.set_parameter(&original, 9.0).unwrap();
    let boxes = world_boxes(&doc);
    assert_eq!(boxes.len(), 2);
    assert!(
        (boxes[0].2.max[2] - 9.0).abs() < 1e-9 && (boxes[1].2.max[2] - 5.0).abs() < 1e-9,
        "{boxes:?}"
    );
    assert!(near(boxes[1].2.min, [50.0, 0.0, 0.0]));
    assert_eq!(doc.assembly().occurrence(o2).unwrap().component, copy);
    let _ = body;

    // A component made by a new-component extrude: the extrude is copied
    // and makes the copy.
    let mut b = block();
    let uid = b
        .doc
        .add_feature(
            &extrude(b.sketch, &[&b.region], 5.0, "new_component", &[]),
            None,
        )
        .unwrap()
        .uid;
    let made = b.doc.assembly().created_by(uid).unwrap();
    let occurrence = b.doc.assembly().occurrences_of(made).next().unwrap().uid;
    let (copy, _) = b.doc.paste_new(occurrence, None).unwrap();
    let maker = b
        .doc
        .assembly()
        .component(copy)
        .unwrap()
        .created_by
        .unwrap();
    assert_ne!(maker, uid);
    assert_eq!(b.doc.feature(maker).unwrap().component, ComponentUid::ROOT);
    assert_eq!(b.doc.body_component(BodyUid::new(maker, 0)), Some(copy));
    assert_eq!(b.doc.assembly().name(copy), "Component1 (1)");
}

#[test]
fn deleting_the_last_occurrence_deletes_the_component() {
    let mut doc = Document::new(MockKernel::default());
    let (component, o1) = doc
        .create_component(None, Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut doc);
    let (_, inner) = doc
        .create_component(Some("Inner"), Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let o2 = doc.copy_occurrence(o1, None).unwrap();
    assert!(doc.delete_occurrence(o2).unwrap().is_empty());
    assert!(doc.assembly().exists(component));
    let deleted = doc.delete_occurrence(o1).unwrap();
    assert_eq!(deleted.len(), 4);
    assert!(doc.assembly().is_empty(), "{:?}", doc.assembly());
    assert!(doc.assembly().occurrence(inner).is_none());
    assert_eq!(doc.features().count(), 0);
}

#[test]
fn components_survive_the_design_file() {
    let mut doc = Document::new(MockKernel::default());
    small_block(&mut doc);
    let (_, o1) = doc
        .create_component(Some("Arm"), shift(1.0, 2.0, 3.0), true)
        .unwrap();
    small_block(&mut doc);
    doc.set_occurrence_grounded(o1, true).unwrap();
    let (_, o2) = doc
        .create_component(Some("Pin"), Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut doc);
    doc.set_occurrence_visible(o2, false).unwrap();
    doc.rename_component(ComponentUid::ROOT, "Assembly")
        .unwrap();
    let json = doc.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(file["root_component"], "Assembly");
    assert_eq!(file["active_component"], "C2");
    assert_eq!(
        file["occurrences"][0],
        json!({"uid": "O1", "component": "C1", "parent": "C0", "number": 1,
               "transform": [[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 2.0], [0.0, 0.0, 1.0, 3.0]],
               "grounded": true})
    );
    assert_eq!(file["occurrences"][1]["visible"], false);
    assert_eq!(file["features"][2]["component"], "C1");
    assert_eq!(file["features"][0].get("component"), None);

    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    loaded.recompute();
    assert_eq!(loaded.instances().len(), 3);
    assert_eq!(loaded.instances().iter().filter(|i| i.visible).count(), 2);
    // New ids continue after the loaded ones.
    let (c, o) = loaded
        .create_component(None, Transform::IDENTITY, false)
        .unwrap();
    assert_eq!((c, o), (ComponentUid(3), OccurrenceUid(3)));

    // Broken structure is refused with its place.
    for (change, expected) in [
        (
            json!({"occurrences": [{"uid": "O1", "component": "C9", "parent": "C0", "number": 1}]}),
            "component C9 does not exist",
        ),
        (
            json!({"active_component": "C7"}),
            "the active component C7 does not exist",
        ),
        (
            json!({"occurrences": [{"uid": "O1", "component": "C1", "parent": "C0", "number": 1,
                                 "transform": [[2, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0]]}]}),
            "rotation and a translation",
        ),
    ] {
        let mut broken = file.clone();
        for (key, value) in change.as_object().unwrap() {
            broken[key] = value.clone();
        }
        let error = Document::from_json(&broken.to_string(), MockKernel::default())
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn files_without_components_load_into_the_root() {
    let b = block();
    let mut file: Value = serde_json::from_str(&b.doc.to_json()).unwrap();
    assert_eq!(file.get("components"), None);
    file.as_object_mut().unwrap().remove("bodies");
    let mut doc = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    doc.recompute();
    assert!(doc.assembly().is_empty());
    assert!(doc.features().all(|f| f.component.is_root()));
    assert_eq!(doc.body_component(b.body), Some(ComponentUid::ROOT));
    assert_eq!(doc.instances().len(), 1);
    assert!(doc.instances()[0].transform.is_identity());
}

#[test]
fn copy_feature_definitions_rename_their_references() {
    let mut doc = Document::new(MockKernel::default());
    let (_, o1) = doc
        .create_component(None, Transform::IDENTITY, true)
        .unwrap();
    let (extrude_uid, body) = small_block(&mut doc);
    let edge = format!("E{{{0}:side(c1[c4,c2])|{0}:side(c2[c1,c3])}}", extrude_uid);
    doc.add_feature(
        &def(json!({"type": "fillet", "body": body, "edges": [edge], "radius": "d1 / 4"})),
        None,
    )
    .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (copy, _) = doc.paste_new(o1, None).unwrap();
    let fillet = doc
        .features()
        .filter(|f| f.component == copy)
        .last()
        .unwrap()
        .clone();
    let value = query(&doc, json!({"query": "feature", "uid": fillet.uid}));
    let copied_extrude = doc
        .features()
        .filter(|f| f.component == copy)
        .nth(1)
        .unwrap()
        .uid;
    assert_eq!(value["def"]["body"], format!("{copied_extrude}.b0"));
    let edges = value["def"]["edges"][0].as_str().unwrap().to_owned();
    assert!(edges.contains(&format!("{copied_extrude}:side")), "{edges}");
    // The fillet radius expression follows the copied sketch's parameter.
    let radius = value["def"]["radius"].as_str().unwrap().to_owned();
    let p = doc
        .parameters()
        .find(&radius)
        .map(|id| doc.parameters().get(id).unwrap().expression().to_owned());
    assert!(
        p.as_deref()
            .is_some_and(|e| e.ends_with("/ 4") && !e.starts_with("d1 ")),
        "{p:?}"
    );
    assert_eq!(doc.status(fillet.uid), Some(&FeatureStatus::Ok));
}

#[test]
fn external_components_link_or_copy_other_files() {
    let dir = std::env::temp_dir().join(format!("mitcad-f6-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut part = Document::new(MockKernel::default());
    small_block(&mut part);
    let path = dir.join("part.mitcad");
    std::fs::write(&path, part.to_json()).unwrap();

    let mut doc = Document::new(MockKernel::default());
    let options = InsertOptions {
        transform: shift(0.0, 0.0, 100.0),
        base: Some(dir.clone()),
        ..InsertOptions::default()
    };
    let (linked, _) = doc.insert_component("part.mitcad", &options).unwrap();
    assert_eq!(doc.assembly().name(linked), "part");
    let link = doc
        .assembly()
        .component(linked)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert_eq!(link.path, "part.mitcad");
    assert_eq!(doc.feature(link.feature).unwrap().def.type_name(), "base");
    // Read only.
    let error = doc.activate_component(linked).unwrap_err().to_string();
    assert!(error.contains("linked from part.mitcad"), "{error}");
    assert!(doc.delete_feature(link.feature, false).is_err());
    let boxes = world_boxes(&doc);
    assert_eq!(boxes.len(), 1);
    assert!((boxes[0].2.min[2] - 100.0).abs() < 1e-9);

    // A copy: the file's history, editable.
    let copy_options = InsertOptions {
        link: false,
        base: Some(dir.clone()),
        ..InsertOptions::default()
    };
    let (copied, _) = doc.insert_component("part.mitcad", &copy_options).unwrap();
    assert_eq!(doc.assembly().name(copied), "part (1)");
    let types: Vec<&str> = doc
        .features()
        .filter(|f| f.component == copied)
        .map(|f| f.def.type_name())
        .collect();
    assert_eq!(types, ["sketch", "extrude"]);

    // The file changes: reopening takes its new bodies.
    let saved = doc.to_json();
    let mut unchanged = Document::from_json(&saved, MockKernel::default()).unwrap();
    let revision = unchanged.revision();
    let messages = unchanged.update_links(Some(&dir)).unwrap();
    assert!(messages.is_empty(), "{messages:?}");
    assert_eq!(unchanged.revision(), revision);
    small_block(&mut part);
    std::fs::write(&path, part.to_json()).unwrap();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    let messages = reopened.update_links(Some(&dir)).unwrap();
    assert_eq!(messages, ["part is updated from part.mitcad"]);
    // Changed without an undo step, so with a new revision (P8).
    assert_ne!(reopened.revision(), revision);
    assert_eq!(reopened.undo_depth(), 0);
    reopened.recompute();
    assert_eq!(reopened.component_bodies(linked).len(), 2);
    // A missing file keeps the saved bodies.
    std::fs::remove_file(&path).unwrap();
    let mut missing = Document::from_json(&saved, MockKernel::default()).unwrap();
    let messages = missing.update_links(Some(&dir)).unwrap();
    assert!(
        messages[0].contains("cannot read part.mitcad"),
        "{messages:?}"
    );
    missing.recompute();
    assert_eq!(missing.component_bodies(linked).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The link of a component by its name, as the design JSON holds it.
fn set_link_path(json: &str, component: &str, path: &str) -> String {
    let mut value: Value = serde_json::from_str(json).unwrap();
    let c = value["components"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["name"] == component)
        .unwrap();
    c["link"]["path"] = json!(path);
    value.to_string()
}

#[test]
fn only_links_in_the_design_project_are_followed() {
    // mitcad#929: the design file does not choose which files outside its
    // project the application reads as the design opens.
    let dir = std::env::temp_dir().join(format!("mitcad-929-links-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = dir.join("project");
    mitcad_model::Project::init(&project).unwrap();
    std::fs::create_dir_all(project.join("parts")).unwrap();
    std::fs::create_dir_all(project.join("designs")).unwrap();
    let designs = project.join("designs");
    let mut part = Document::new(MockKernel::default());
    small_block(&mut part);
    let inside = project.join("parts").join("part.mitcad");
    std::fs::write(&inside, part.to_json()).unwrap();
    // A file beside the project, outside it.
    let far = dir.join("far.mitcad");
    std::fs::write(&far, part.to_json()).unwrap();

    // Inserted by absolute paths (as the application inserts): a file in
    // the project is recorded from the design's folder, one outside as
    // given.
    let mut doc = Document::new(MockKernel::default());
    let options = InsertOptions {
        base: Some(designs.clone()),
        ..InsertOptions::default()
    };
    let (linked, _) = doc
        .insert_component(inside.to_str().unwrap(), &options)
        .unwrap();
    let (outside, _) = doc
        .insert_component(far.to_str().unwrap(), &options)
        .unwrap();
    let path_of = |doc: &Document<MockKernel>, c: ComponentUid| {
        doc.assembly()
            .component(c)
            .unwrap()
            .link
            .clone()
            .unwrap()
            .path
    };
    assert_eq!(path_of(&doc, linked), "../parts/part.mitcad");
    assert_eq!(path_of(&doc, outside), far.to_str().unwrap());
    assert_eq!(doc.assembly().name(outside), "far");
    let saved = doc.to_json();

    // Both files change: only the one in the project is read again.
    small_block(&mut part);
    std::fs::write(&inside, part.to_json()).unwrap();
    std::fs::write(&far, part.to_json()).unwrap();
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    let messages = reopened.update_links(Some(&designs)).unwrap();
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert_eq!(messages[0], "part is updated from ../parts/part.mitcad");
    assert_eq!(
        messages[1],
        format!(
            "far: the link outside the project is not followed ({}); it keeps the bodies \
             saved with the project",
            far.display()
        )
    );
    reopened.recompute();
    assert_eq!(reopened.component_bodies(linked).len(), 2);
    assert_eq!(reopened.component_bodies(outside).len(), 1);

    // A `..` out of the project: refused before anything is read (the file
    // does not exist, and the message is not a read error).
    for path in ["../../outside.mitcad", "/no/such/folder/outside.mitcad"] {
        let json = set_link_path(&saved, "part", path);
        let mut opened = Document::from_json(&json, MockKernel::default()).unwrap();
        let messages = opened.update_links(Some(&designs)).unwrap();
        assert!(
            messages[0].contains("link outside the project is not followed")
                && messages[0].contains(path)
                && !messages[0].contains("cannot read"),
            "{messages:?}"
        );
        opened.recompute();
        assert_eq!(opened.component_bodies(linked).len(), 1);
    }
    // A `..` that stays in the project is followed.
    let json = set_link_path(&saved, "part", "../designs/../parts/./part.mitcad");
    let mut opened = Document::from_json(&json, MockKernel::default()).unwrap();
    let messages = opened.update_links(Some(&designs)).unwrap();
    assert_eq!(
        messages[0],
        "part is updated from ../designs/../parts/./part.mitcad"
    );
    // A design without a folder follows no link.
    let mut unsaved = Document::from_json(&saved, MockKernel::default()).unwrap();
    let messages = unsaved.update_links(None).unwrap();
    assert_eq!(messages.len(), 2, "{messages:?}");
    assert!(
        messages
            .iter()
            .all(|m| m.contains("link outside the project is not followed")),
        "{messages:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_design_in_no_project_follows_links_in_its_folder() {
    // mitcad#929: without a project the design's folder is the limit.
    let dir = std::env::temp_dir().join(format!("mitcad-929-folder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let designs = dir.join("designs");
    std::fs::create_dir_all(designs.join("sub")).unwrap();
    let mut part = Document::new(MockKernel::default());
    small_block(&mut part);
    let path = designs.join("sub").join("part.mitcad");
    std::fs::write(&path, part.to_json()).unwrap();
    std::fs::write(dir.join("outside.mitcad"), part.to_json()).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let options = InsertOptions {
        base: Some(designs.clone()),
        ..InsertOptions::default()
    };
    let (linked, _) = doc
        .insert_component(path.to_str().unwrap(), &options)
        .unwrap();
    assert_eq!(
        doc.assembly()
            .component(linked)
            .unwrap()
            .link
            .as_ref()
            .unwrap()
            .path,
        "sub/part.mitcad"
    );
    let saved = set_link_path(&doc.to_json(), "part", "../outside.mitcad");
    let mut opened = Document::from_json(&saved, MockKernel::default()).unwrap();
    let messages = opened.update_links(Some(&designs)).unwrap();
    assert!(
        messages[0].contains("link outside the project is not followed (../outside.mitcad)"),
        "{messages:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn placements_read_and_write_as_rows() {
    let t = shift(1.0, 2.0, 3.0);
    let rows: Vec<Vec<f64>> = matrix_rows(&t).iter().map(|r| r.to_vec()).collect();
    assert_eq!(from_rows(&rows).unwrap(), t);
}

#[test]
fn another_documents_design_is_copied_into_a_component() {
    // A part made in a document of its own (as the .iam import makes each
    // part), copied into a component of the assembly and placed twice.
    let mut part = Document::new(MockKernel::default());
    let (_, body) = small_block(&mut part);
    assert!(part.body_shape(body).is_some());
    let mut doc = Document::new(MockKernel::default());
    let (component, first) = doc
        .add_component_copy(
            &part,
            Some("block"),
            ComponentUid::ROOT,
            shift(20.0, 0.0, 0.0),
        )
        .unwrap();
    let second = doc
        .add_occurrence(component, ComponentUid::ROOT, shift(0.0, 30.0, 0.0))
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(doc.assembly().name(component), "block");
    // The copy's features are the component's, recomputed in the assembly.
    assert_eq!(doc.component_bodies(component).len(), 1);
    assert!(doc.features().all(|f| f.component == component));
    let boxes = world_boxes(&doc);
    assert_eq!(boxes.len(), 2);
    let mins: Vec<[f64; 3]> = boxes.iter().map(|b| b.2.min).collect();
    assert!(mins.iter().any(|m| near(*m, [20.0, 0.0, 0.0])));
    assert!(mins.iter().any(|m| near(*m, [0.0, 30.0, 0.0])));
    // The active component stays; a second copy gets a name of its own.
    assert_eq!(doc.active_component(), ComponentUid::ROOT);
    let (again, _) = doc
        .add_component_copy(
            &part,
            Some("block"),
            ComponentUid::ROOT,
            Transform::IDENTITY,
        )
        .unwrap();
    assert_ne!(doc.assembly().name(again), "block");
    // A copy into a component that does not exist is refused.
    assert!(
        doc.add_component_copy(&part, None, ComponentUid(999), Transform::IDENTITY)
            .is_err()
    );
}

#[test]
fn import_scopes_leave_the_filled_components_bodies_out_of_the_body_view() {
    // A part with one block, copied into an empty component of another
    // document (what the `.f3d` import does for components of other
    // documents, mitcad#196).
    let mut part = Document::new(MockKernel::default());
    small_block(&mut part);
    let mut doc = Document::new(MockKernel::default());
    let (ext, _) = doc
        .create_component(Some("Ext"), Transform::IDENTITY, false)
        .unwrap();
    doc.fill_component_copy(&part, ext, Some("Part")).unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    small_block(&mut doc);
    doc.recompute();
    assert_eq!(doc.bodies().len(), 2);

    // While the scope is set, the body view has the document's own body
    // only; the scope's has the copy's.
    doc.set_import_scopes(vec![("guid".to_owned(), vec![ext])]);
    assert_eq!(doc.bodies().len(), 1);
    assert_eq!(
        doc.body_component(doc.bodies()[0].uid),
        Some(ComponentUid::ROOT)
    );
    assert_eq!(doc.scope_bodies("guid").len(), 1);
    assert_eq!(doc.scope_bodies("other").len(), 0);
    assert_eq!(doc.import_scope("guid"), vec![ext]);
    // The copy's bodies are still the component's own.
    assert_eq!(doc.component_bodies(ext).len(), 1);
    // A fork has them too.
    if let Some(fork) = doc.fork() {
        assert_eq!(fork.bodies().len(), 1);
    }

    doc.set_import_scopes(Vec::new());
    assert_eq!(doc.bodies().len(), 2);
    assert!(doc.import_scope("guid").is_empty());
}

#[test]
fn an_occurrence_gets_a_component_of_its_own_or_joins_another() {
    // An importer's operations (mitcad#196): occurrences of one component
    // that are to differ get components of their own.
    let mut doc = Document::new(MockKernel::default());
    let (part, first) = doc
        .create_component(Some("Part"), Transform::IDENTITY, false)
        .unwrap();
    let second = doc
        .add_occurrence(part, ComponentUid::ROOT, shift(20.0, 0.0, 0.0))
        .unwrap();
    let third = doc
        .add_occurrence(part, ComponentUid::ROOT, shift(40.0, 0.0, 0.0))
        .unwrap();
    assert_eq!(doc.assembly().occurrence_name(third), "Part:3");

    // The second occurrence gets an empty component of its own and keeps
    // its uid and placement.
    let own = doc
        .give_occurrence_own_component(second, Some("Part"))
        .unwrap();
    assert_ne!(own, part);
    assert_eq!(doc.assembly().name(own), "Part (1)");
    let o = doc.assembly().occurrence(second).unwrap();
    assert_eq!((o.component, o.number), (own, 1));
    assert!(near(
        doc.placement(second).unwrap().translation,
        [20.0, 0.0, 0.0]
    ));
    assert_eq!(doc.assembly().occurrences_of(part).count(), 2);
    assert_eq!(doc.assembly().occurrence_name(second), "Part (1):1");

    // The third joins it and takes the next number.
    doc.place_other_component(third, own).unwrap();
    assert_eq!(doc.assembly().occurrence_name(third), "Part (1):2");
    assert_eq!(doc.assembly().occurrences_of(part).count(), 1);
    assert!(doc.assembly().occurrences_of(part).any(|o| o.uid == first));
    assert!(near(
        doc.placement(third).unwrap().translation,
        [40.0, 0.0, 0.0]
    ));

    // Not the root, a component that does not exist, or one that would
    // contain its own parent; one step of undo takes each back.
    assert!(
        doc.place_other_component(third, ComponentUid::ROOT)
            .is_err()
    );
    assert!(doc.place_other_component(third, ComponentUid(99)).is_err());
    let (inner, _) = doc
        .add_component(Some("Inner"), own, Transform::IDENTITY)
        .unwrap();
    let inside = doc.assembly().children(own).next().unwrap().uid;
    assert!(doc.place_other_component(inside, own).is_err(), "{inner:?}");
    doc.undo().unwrap();
    assert_eq!(doc.assembly().occurrence_name(third), "Part (1):2");
}

#[test]
fn make_unique_copies_the_component_of_one_occurrence() {
    let mut doc = Document::new(MockKernel::default());
    let (part, first) = doc
        .create_component(Some("Part"), Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let second = doc
        .copy_occurrence(first, Some(shift(50.0, 0.0, 0.0)))
        .unwrap();
    assert_eq!(doc.assembly().occurrences_of(part).count(), 2);

    let (copy, copied) = doc.make_unique(second).unwrap();
    assert_ne!(copy, part);
    assert_eq!(doc.assembly().name(copy), "Part (1)");
    // The occurrence keeps its uid and placement; the original keeps the
    // first.
    let o = doc.assembly().occurrence(second).unwrap();
    assert_eq!((o.component, o.number), (copy, 1));
    assert_eq!(doc.assembly().occurrence(first).unwrap().component, part);
    assert!(near(
        doc.placement(second).unwrap().translation,
        [50.0, 0.0, 0.0]
    ));
    // The copy has its own features and parameters: a change of the
    // original's does not reach it.
    assert_eq!(copied.features.len(), 2);
    assert_eq!(doc.features().filter(|f| f.component == copy).count(), 2);
    let original = doc
        .parameters()
        .iter()
        .find(|p| p.comment() == "Extrude1 distance")
        .unwrap()
        .name()
        .to_owned();
    doc.set_parameter(&original, 9.0).unwrap();
    let boxes = world_boxes(&doc);
    assert_eq!(boxes.len(), 2);
    let heights: Vec<f64> = boxes.iter().map(|b| b.2.max[2]).collect();
    assert!(
        heights.contains(&9.0) && heights.contains(&5.0),
        "{heights:?}"
    );

    // Undo takes it back in one step (after the parameter edit); an
    // occurrence that does not exist is refused.
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.assembly().occurrence(second).unwrap().component, part);
    assert_eq!(doc.assembly().occurrences_of(part).count(), 2);
    assert!(doc.make_unique(OccurrenceUid(999)).is_err());
}

#[test]
fn renaming_a_component_renames_its_only_default_named_body() {
    // mitcad#291: Plate's one body, Body1, takes the component's new name.
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate"}),
    );
    let (_, body) = small_block(&mut doc);
    assert_eq!(doc.body_name(body), "Body1");
    let depth = doc.undo_depth();
    let reply = command(
        &mut doc,
        json!({"cmd": "rename_component", "component": "Plate", "name": "Bracket"}),
    );
    assert_eq!(
        reply["renamed_body"],
        json!({"uid": body, "name": "Bracket"})
    );
    assert_eq!(doc.body_name(body), "Bracket");
    assert_eq!(doc.undo_depth(), depth + 1);
    // One undo step takes both names back.
    assert_eq!(doc.undo().unwrap(), "Rename Plate to Bracket");
    assert_eq!(doc.body_name(body), "Body1");
    assert_eq!(doc.assembly().name(ComponentUid(1)), "Plate");
    doc.redo().unwrap();
    assert_eq!(doc.body_name(body), "Bracket");
    // Named with the component, the body's name is no longer a default
    // one: a later rename leaves it.
    let reply = command(
        &mut doc,
        json!({"cmd": "rename_component", "component": "Bracket", "name": "Holder"}),
    );
    assert_eq!(reply["renamed_body"], Value::Null);
    assert_eq!(doc.body_name(body), "Bracket");
}

#[test]
fn renaming_a_component_leaves_named_bodies_two_bodies_the_root_and_taken_names() {
    let rename = |doc: &mut Document<MockKernel>, component: &str, name: &str| {
        command(
            doc,
            json!({"cmd": "rename_component", "component": component, "name": name}),
        )["renamed_body"]
            .clone()
    };
    // A body the user named.
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate"}),
    );
    let (_, body) = small_block(&mut doc);
    doc.rename_body(body, "Top plate").unwrap();
    assert_eq!(rename(&mut doc, "Plate", "Bracket"), Value::Null);
    assert_eq!(doc.body_name(body), "Top plate");

    // Two bodies.
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate"}),
    );
    let (_, first) = small_block(&mut doc);
    let (_, second) = small_block(&mut doc);
    assert_eq!(rename(&mut doc, "Plate", "Bracket"), Value::Null);
    assert_eq!(
        (doc.body_name(first), doc.body_name(second)),
        ("Body1".to_owned(), "Body2".to_owned())
    );

    // The root.
    let mut doc = Document::new(MockKernel::default());
    let (_, body) = small_block(&mut doc);
    assert_eq!(rename(&mut doc, "Root", "Assembly"), Value::Null);
    assert_eq!(doc.body_name(body), "Body1");

    // A name another body already has.
    let mut doc = Document::new(MockKernel::default());
    let (_, root_body) = small_block(&mut doc);
    doc.rename_body(root_body, "Bracket").unwrap();
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate"}),
    );
    let (_, body) = small_block(&mut doc);
    let default = doc.body_name(body);
    assert!(default.starts_with("Body"), "{default}");
    assert_eq!(rename(&mut doc, "Plate", "Bracket"), Value::Null);
    assert_eq!(doc.body_name(body), default);
    assert_eq!(doc.assembly().name(ComponentUid(1)), "Bracket");
}

#[test]
fn activating_as_the_document_opens_is_no_undo_step_and_no_change() {
    // A design saved with a component active (mitcad#426).
    let mut doc = Document::new(MockKernel::default());
    small_block(&mut doc);
    doc.create_component(Some("Bracket"), Transform::IDENTITY, true)
        .unwrap();
    small_block(&mut doc);
    let json = doc.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(file["active_component"], "C1");

    // Opened and marked saved as the application does, then the root
    // activated: unmodified, no undo step, the same revision.
    let mut opened = Document::from_json(&json, MockKernel::default()).unwrap();
    opened.recompute();
    command(&mut opened, json!({"cmd": "mark_saved"}));
    let document = query(&opened, json!({"query": "document"}));
    let reply = command(
        &mut opened,
        json!({"cmd": "activate_component", "component": "C0", "opening": true}),
    );
    assert_eq!(reply["component"], "C0");
    assert_eq!(reply["previous"], "C1");
    assert_eq!(reply["recomputed"], 0);
    assert_eq!(opened.active_component(), ComponentUid::ROOT);
    let after = query(&opened, json!({"query": "document"}));
    assert_eq!(after["modified"], false);
    assert_eq!(after["revision"], document["revision"]);
    assert_eq!(opened.undo_depth(), 0);
    // Saving stores the root active.
    let saved: Value = serde_json::from_str(&opened.to_json()).unwrap();
    assert_eq!(saved.get("active_component"), None);

    // A plain activation is still an undo step.
    command(
        &mut opened,
        json!({"cmd": "activate_component", "component": "Bracket"}),
    );
    assert_eq!(opened.undo_depth(), 1);
    assert_eq!(
        query(&opened, json!({"query": "document"}))["modified"],
        true
    );
    // A component that does not exist is refused.
    let error = opened
        .activate_component_on_open(ComponentUid(9))
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not exist"), "{error}");
}

/// A 10 x 10 x 10 box at the origin of the active component, with the
/// vertex at its corner (10, 0, 10) set in the mock kernel: the box's body
/// and the vertex's name.
fn box_with_vertex(doc: &mut Document<MockKernel>) -> (BodyUid, String) {
    let uid = doc
        .add_feature(
            &def(
                json!({"type": "box", "plane": "xy", "corner": [0, 0], "length": 10,
                        "width": 10, "height": 10, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    let vertex = format!("V{{{uid}:side0|{uid}:side1|{uid}:top}}")
        .parse::<mitcad_model::topo::VertexName>()
        .unwrap()
        .to_string();
    doc.kernel()
        .vertex_points
        .borrow_mut()
        .insert(vertex.clone(), [10.0, 0.0, 10.0]);
    (BodyUid::new(uid, 0), vertex)
}

fn assert_ok(doc: &Document<MockKernel>, uid: FeatureUid) {
    let status = doc.status(uid).unwrap();
    assert!(matches!(status, FeatureStatus::Ok), "{status:?}");
}

/// Point to Point takes each point where its occurrence places it
/// (mitcad#699): a vertex of A to a vertex of B, both components placed
/// away from the origin, puts the two vertices together.
#[test]
fn a_move_of_an_occurrence_takes_points_where_their_occurrences_place_them() {
    let mut doc = Document::new(MockKernel::default());
    let (_, a) = doc
        .create_component(Some("A"), shift(100.0, 0.0, 0.0), true)
        .unwrap();
    let (a_body, a_vertex) = box_with_vertex(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (_, b) = doc
        .create_component(Some("B"), shift(0.0, 50.0, 20.0), true)
        .unwrap();
    let (b_body, b_vertex) = box_with_vertex(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    // A's vertex is at (110, 0, 10) in the design, B's at (10, 50, 30).
    let moved = doc
        .add_feature(
            &def(json!({"type": "move_occurrence", "occurrences": [a],
                "transform": {"type": "point_to_point",
                    "from": {"body": a_body, "vertex": a_vertex}, "from_occurrence": a.to_string(),
                    "to": {"body": b_body, "vertex": b_vertex}, "to_occurrence": b.to_string()}})),
            None,
        )
        .unwrap();
    assert_ok(&doc, moved.uid);
    assert!(
        near(doc.placement(a).unwrap().translation, [0.0, 50.0, 20.0]),
        "{:?}",
        doc.placement(a)
    );
    assert!(near(
        doc.placement(b).unwrap().translation,
        [0.0, 50.0, 20.0]
    ));
    // The occurrences are kept as written, and come back from the file.
    let saved = query(&doc, json!({"query": "feature", "uid": moved.uid}));
    assert_eq!(saved["def"]["transform"]["from_occurrence"], a.to_string());
    assert_eq!(saved["def"]["transform"]["to_occurrence"], b.to_string());
    // The reopened document's kernel knows the vertices as the first one does.
    let kernel = MockKernel::default();
    for vertex in [&a_vertex, &b_vertex] {
        kernel
            .vertex_points
            .borrow_mut()
            .insert(vertex.clone(), [10.0, 0.0, 10.0]);
    }
    let mut reopened = Document::from_json(&doc.to_json(), kernel).unwrap();
    reopened.recompute();
    assert!(near(
        reopened.placement(a).unwrap().translation,
        [0.0, 50.0, 20.0]
    ));
    // B placed elsewhere: the move follows it.
    doc.set_occurrence_transform(b, shift(0.0, 0.0, 5.0), false)
        .unwrap();
    assert!(near(doc.placement(a).unwrap().translation, [0.0, 0.0, 5.0]));
}

/// A component placed twice: the vertex of the second copy names that
/// copy, so a copy can be moved onto the other one (mitcad#699).
#[test]
fn a_move_of_an_occurrence_tells_the_copies_of_a_component_apart() {
    let mut doc = Document::new(MockKernel::default());
    let (_, first) = doc
        .create_component(Some("Part"), shift(100.0, 0.0, 0.0), true)
        .unwrap();
    let (body, vertex) = box_with_vertex(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let second = doc
        .copy_occurrence(first, Some(shift(0.0, 40.0, 0.0)))
        .unwrap();
    // The vertex is at (110, 0, 10) in Part:1 and at (10, 40, 10) in Part:2.
    let point = json!({"body": body, "vertex": vertex});
    let moved = doc
        .add_feature(
            &def(json!({"type": "move_occurrence", "occurrences": [second],
                "transform": {"type": "point_to_point",
                    "from": point, "from_occurrence": second.to_string(),
                    "to": point, "to_occurrence": first.to_string()}})),
            None,
        )
        .unwrap();
    assert_ok(&doc, moved.uid);
    assert!(
        near(
            doc.placement(second).unwrap().translation,
            [100.0, 0.0, 0.0]
        ),
        "{:?}",
        doc.placement(second)
    );
    assert!(near(
        doc.placement(first).unwrap().translation,
        [100.0, 0.0, 0.0]
    ));
    // Point to Position with the point in the second copy, where the move
    // before put it: (110, 0, 10) to the origin.
    let positioned = doc
        .add_feature(
            &def(json!({"type": "move_occurrence", "occurrences": [second],
                "transform": {"type": "point_to_position", "point": point,
                    "point_occurrence": second.to_string(), "x": 0, "y": 0, "z": 0}})),
            None,
        )
        .unwrap();
    assert_ok(&doc, positioned.uid);
    assert!(near(
        doc.placement(second).unwrap().translation,
        [-10.0, 0.0, -10.0]
    ));
}

/// Sketches and construction geometry of another component are taken
/// where an occurrence places them; without one they are still refused
/// (mitcad#699).
#[test]
fn a_move_takes_a_sketch_point_of_another_component_by_its_occurrence() {
    let mut doc = Document::new(MockKernel::default());
    let (_, a) = doc
        .create_component(Some("A"), shift(100.0, 0.0, 0.0), true)
        .unwrap();
    box_with_vertex(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (_, b) = doc
        .create_component(Some("B"), shift(0.0, 50.0, 0.0), true)
        .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    command(
        &mut doc,
        json!({"cmd": "sketch.add_point", "sketch": sketch, "at": [5, 5]}),
    );
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let point = json!({"sketch": sketch, "point": "p1"});
    let error = doc
        .add_feature(
            &def(json!({"type": "move_occurrence", "occurrences": [a],
                "transform": {"type": "point_to_point",
                    "from": {"point": [100, 0, 0]}, "to": point}})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("is in B"), "{error}");
    let moved = doc
        .add_feature(
            &def(json!({"type": "move_occurrence", "occurrences": [a],
                "transform": {"type": "point_to_point",
                    "from": {"point": [100, 0, 0]}, "to": point,
                    "to_occurrence": b.to_string()}})),
            None,
        )
        .unwrap();
    assert_ok(&doc, moved.uid);
    // The sketch point is at (5, 55, 0) in the design.
    assert!(near(
        doc.placement(a).unwrap().translation,
        [5.0, 55.0, 0.0]
    ));
}

/// A definition without occurrences (earlier versions, imports) means
/// what it meant: its points in the feature's component (mitcad#699).
#[test]
fn a_move_without_occurrences_takes_its_points_in_its_component_as_before() {
    let mut doc = Document::new(MockKernel::default());
    let (root_body, root_vertex) = box_with_vertex(&mut doc);
    let (_, a) = doc
        .create_component(Some("A"), shift(100.0, 0.0, 0.0), true)
        .unwrap();
    let (a_body, a_vertex) = box_with_vertex(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let moved = doc
        .add_feature(
            &def(json!({"type": "move_occurrence", "occurrences": [a],
                "transform": {"type": "point_to_point", "from": {"point": [100, 0, 0]},
                    "to": {"body": root_body, "vertex": root_vertex}}})),
            None,
        )
        .unwrap();
    assert_ok(&doc, moved.uid);
    assert!(near(
        doc.placement(a).unwrap().translation,
        [10.0, 0.0, 10.0]
    ));
    // Written back without occurrences.
    let saved = query(&doc, json!({"query": "feature", "uid": moved.uid}));
    let transform = saved["def"]["transform"].as_object().unwrap();
    assert!(!transform.contains_key("from_occurrence"), "{transform:?}");
    assert!(!transform.contains_key("to_occurrence"), "{transform:?}");
    // A body of another component without its occurrence fails, as before.
    let other = doc
        .add_feature(
            &def(json!({"type": "move_occurrence", "occurrences": [a],
                "transform": {"type": "point_to_point",
                    "from": {"body": a_body, "vertex": a_vertex},
                    "to": {"point": [0, 0, 0]}}})),
            None,
        )
        .unwrap();
    let status = doc.status(other.uid).unwrap();
    assert!(
        matches!(status, FeatureStatus::Failed(m) if m.contains("belongs to A")),
        "{status:?}"
    );
}
