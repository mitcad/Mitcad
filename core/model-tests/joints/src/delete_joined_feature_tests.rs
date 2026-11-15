// SPDX-License-Identifier: MIT
//! Deleting a feature whose body another occurrence is joined to
//! (mitcad#1820), with the mock kernel: a plate (a block in a component of
//! its own) and a pin joined to the plate's top. Deleting the plate's
//! extrude or its sketch with their dependents takes the joint along, as
//! before, but the pin stays where the joint put it (within 1e-6 mm)
//! instead of falling back to its stored placement, also with a Move of it
//! on the timeline; undo brings the feature, the joint and the pin's own
//! placement back. A joint deleted by itself still lets its part go back
//! where it was placed.
//!
//! The mock's blocks are 10 x 10 x 5 at their component's origin: the
//! planar end face's middle is (5, 5, 5) with the normal +Z, the start
//! face's (5, 5, 0) with -Z.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

/// A 10 x 10 x 5 block in the active component: its sketch, its extrude
/// and its body.
fn block_in(doc: &mut Document<MockKernel>) -> (FeatureUid, FeatureUid, BodyUid) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (sketch, uid, BodyUid::new(uid, 0))
}

fn face(doc: &Document<MockKernel>, body: BodyUid, role: &str) -> String {
    doc.body_shape(body)
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == role)
        .map(ToString::to_string)
        .unwrap()
}

/// Within 1e-6 mm (and its axes within 1e-6).
fn same(a: &Transform, b: &Transform) -> bool {
    (0..3).all(|i| (a.translation[i] - b.translation[i]).abs() < 1e-6)
        && (0..3).all(|r| (0..3).all(|c| (a.linear[r][c] - b.linear[r][c]).abs() < 1e-6))
}

fn joints(doc: &Document<MockKernel>) -> usize {
    doc.features()
        .filter(|e| matches!(e.def, FeatureDef::Joint(_)))
        .count()
}

/// The plate (grounded or not) at the origin and the pin at x = 100,
/// joined bottom on top: the plate's sketch and extrude, the pin's
/// occurrence and the joint.
struct Rig {
    doc: Document<MockKernel>,
    plate: OccurrenceUid,
    sketch: FeatureUid,
    extrude: FeatureUid,
    pin: OccurrenceUid,
    joint: FeatureUid,
}

fn rig(grounded: bool, moved: bool) -> Rig {
    let mut doc = Document::new(MockKernel::default());
    let (_, plate) = doc
        .create_component(Some("Plate"), Transform::IDENTITY, true)
        .unwrap();
    let (sketch, extrude, plate_body) = block_in(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (_, pin) = doc
        .create_component(Some("Pin"), Transform::translation([100.0, 0.0, 0.0]), true)
        .unwrap();
    let (_, _, pin_body) = block_in(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    if grounded {
        doc.set_occurrence_grounded(plate, true).unwrap();
    }
    if moved {
        // A Move of the pin on the timeline before the joint: what the
        // pin keeps must take it into account.
        command(
            &mut doc,
            json!({"cmd": "add_feature",
                   "def": {"type": "move_occurrence", "occurrences": [pin.to_string()],
                           "transform": {"type": "translate_xyz", "x": 0, "y": 30, "z": -7}}}),
        );
    }
    let pin_face = face(&doc, pin_body, "start");
    let plate_face = face(&doc, plate_body, "end");
    let answer = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid", "flip": true,
               "a": {"occurrence": "Pin:1",
                     "geometry": {"body": pin_body, "face": pin_face}},
               "b": {"occurrence": "Plate:1",
                     "geometry": {"body": plate_body, "face": plate_face}}}),
    );
    assert_eq!(answer["error"], Value::Null, "{answer}");
    let joint = answer["uid"].as_str().unwrap().parse().unwrap();
    Rig {
        doc,
        plate,
        sketch,
        extrude,
        pin,
        joint,
    }
}

/// Deletes `feature` with its dependents and checks: the joint went with
/// it, the plate and the pin stay where they were; undo brings all back.
fn delete_keeps_the_pin(rig: &mut Rig, feature: FeatureUid) {
    let pin_at = rig.doc.placement(rig.pin).unwrap();
    let plate_at = rig.doc.placement(rig.plate).unwrap();
    let stored = rig.doc.assembly().occurrence(rig.pin).unwrap().transform;
    // The joint placed the pin on the plate's top.
    assert!((pin_at.translation[2] - 5.0).abs() < 1e-6, "{pin_at:?}");
    let deleted = command(
        &mut rig.doc,
        json!({"cmd": "delete_feature", "uid": feature, "dependents": true}),
    );
    let deleted = deleted["deleted"].as_array().unwrap().clone();
    assert!(
        deleted.contains(&json!(rig.joint.to_string())),
        "{deleted:?}"
    );
    assert_eq!(joints(&rig.doc), 0, "a joint to what went is left");
    assert!(rig.doc.assembly().occurrence(rig.pin).is_some());
    let now = rig.doc.placement(rig.pin).unwrap();
    assert!(
        same(&now, &pin_at),
        "the pin moved: {now:?}, was {pin_at:?}"
    );
    let plate_now = rig.doc.placement(rig.plate).unwrap();
    assert!(
        same(&plate_now, &plate_at),
        "{plate_now:?}, was {plate_at:?}"
    );
    // Undo: the feature, the joint and the pin's own placement back.
    command(&mut rig.doc, json!({"cmd": "undo"}));
    assert!(rig.doc.feature(feature).is_some());
    assert!(rig.doc.feature(rig.joint).is_some());
    assert_eq!(joints(&rig.doc), 1);
    let back = rig.doc.assembly().occurrence(rig.pin).unwrap().transform;
    assert!(same(&back, &stored), "{back:?}, was {stored:?}");
    assert!(same(&rig.doc.placement(rig.pin).unwrap(), &pin_at));
}

#[test]
fn deleting_the_body_a_part_is_joined_to_leaves_the_part_where_it_is() {
    for grounded in [true, false] {
        let mut rig = rig(grounded, false);
        let extrude = rig.extrude;
        let sketch = rig.sketch;
        delete_keeps_the_pin(&mut rig, extrude);
        // Its sketch, which takes the extrude and the joint along.
        delete_keeps_the_pin(&mut rig, sketch);
    }
}

#[test]
fn a_move_of_the_joined_part_on_the_timeline_is_taken_into_account() {
    let mut rig = rig(true, true);
    let extrude = rig.extrude;
    delete_keeps_the_pin(&mut rig, extrude);
}

#[test]
fn a_joint_deleted_itself_still_lets_its_part_go_back() {
    let mut rig = rig(true, false);
    let stored = rig.doc.assembly().occurrence(rig.pin).unwrap().transform;
    command(
        &mut rig.doc,
        json!({"cmd": "delete_feature", "uid": rig.joint, "dependents": true}),
    );
    assert_eq!(joints(&rig.doc), 0);
    let now = rig.doc.placement(rig.pin).unwrap();
    assert!(same(&now, &stored), "{now:?}, was {stored:?}");
}
