// SPDX-License-Identifier: MIT
//! Components and occurrences (F6) with the mock kernel: shared
//! definitions, placements, nesting, the active component, new-component
//! operations, timeline moves, copies, external components and the
//! project file.

use serde_json::{Value, json};

use crate::assembly::{from_rows, matrix_rows};
use crate::document_tests::{block, def, extrude, num};
use crate::features::SketchPlane;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::{BoundingBox, Kernel};
use crate::testing::MockKernel;
use crate::transform::Transform;
use crate::{Document, FeatureStatus, InsertOptions};

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
fn components_survive_the_project_file() {
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

#[test]
fn placements_read_and_write_as_rows() {
    let t = shift(1.0, 2.0, 3.0);
    let rows: Vec<Vec<f64>> = matrix_rows(&t).iter().map(|r| r.to_vec()).collect();
    assert_eq!(from_rows(&rows).unwrap(), t);
}
