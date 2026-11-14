// SPDX-License-Identifier: MIT
//! An automatic assembly cut leaves alone what moves against its own
//! occurrence through joints (mitcad#1667): a body cut by a part that turns
//! against it would change with the mechanism's pose. An explicit scope
//! still cuts what the user chose.
//!
//! Four stacked components, each a 10 mm box, 20 mm apart along z: A
//! (grounded), B (an as-built revolute joint to A), C (no joint) and D (an
//! as-built rigid joint to A). As-built joints keep the placements, so the
//! cut's tool goes through all four boxes wherever they are.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, Transform};
use mitcad_model_api::DocumentApi;

/// The text of a target's row an automatic scope leaves alone.
const MOVES: &str =
    "Moves against this component through joints, so it is left out; check it to cut it as well.";

struct Rig {
    doc: Document<MockKernel>,
    /// The components and their occurrences, A to D.
    components: [(ComponentUid, OccurrenceUid); 4],
    bodies: [BodyUid; 4],
    /// A rectangle of the root's sketch through the four boxes.
    profile: Value,
}

fn box_body(doc: &mut Document<MockKernel>) -> BodyUid {
    let uid = doc
        .add_feature(
            &def(json!({"type": "box", "plane": "xy", "corner": [0, 0],
                        "length": 10, "width": 10, "height": 10, "operation": "new_body"})),
            None,
        )
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn rig() -> Rig {
    let mut doc = Document::new(MockKernel::default());
    let mut components = Vec::new();
    let mut bodies = Vec::new();
    for (i, name) in ["A", "B", "C", "D"].into_iter().enumerate() {
        doc.activate_component(ComponentUid::ROOT).unwrap();
        components.push(
            doc.create_component(
                Some(name),
                Transform::translation([0.0, 0.0, i as f64 * 20.0]),
                true,
            )
            .unwrap(),
        );
        bodies.push(box_body(&mut doc));
    }
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_grounded(components[0].1, true).unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    Rig {
        doc,
        components: components.try_into().unwrap(),
        bodies: bodies.try_into().unwrap(),
        profile: json!([{"sketch": sketch, "region": region}]),
    }
}

impl Rig {
    fn path(&self, i: usize) -> String {
        self.components[i].1.to_string()
    }

    /// A cut from the root's sketch through all four boxes with `targets`.
    fn cut(&self, targets: Value) -> Value {
        json!({"type": "extrude", "profiles": self.profile,
               "extent": {"type": "distance", "distance": 80}, "operation": "cut",
               "cut_scope": {"context": {"component": "C0", "occurrence": ""},
                             "targets": targets}})
    }

    fn add(&mut self, definition: &Value) -> FeatureUid {
        self.doc
            .add_feature(&def(definition.clone()), None)
            .unwrap()
            .uid
    }

    /// The as-built joints: B turns on A, D is fixed to A.
    fn joints(&mut self) -> [FeatureUid; 2] {
        let (a, b, d) = (self.path(0), self.path(1), self.path(3));
        let mut joint = |kind: &str, one: &str, other: &str| -> FeatureUid {
            let result = command(
                &mut self.doc,
                json!({"cmd": "add_as_built_joint", "kind": kind, "a": one, "b": other}),
            );
            assert_eq!(result["error"], Value::Null, "{result}");
            result["uid"].as_str().unwrap().parse().unwrap()
        };
        [joint("revolute", &b, &a), joint("rigid", &d, &a)]
    }

    fn cut_bodies(&self) -> [bool; 4] {
        self.bodies
            .map(|b| self.doc.body_shape(b).unwrap().history.starts_with("cut("))
    }
}

#[test]
fn an_automatic_cut_leaves_alone_what_turns_against_it() {
    let mut rig = rig();
    rig.joints();
    let cut = rig.cut(json!({"type": "automatic"}));
    let report = rig.doc.preview_add(&def(cut.clone()), None).unwrap();
    assert_eq!(report.status, FeatureStatus::Ok);
    let targets = report.cut_targets.unwrap();
    let row = |component: ComponentUid| {
        targets
            .rows
            .iter()
            .find(|r| r.component == component)
            .unwrap()
            .clone()
    };
    // B turns on A: it intersects, but is left alone with the reason.
    let b = row(rig.components[1].0);
    assert!(b.intersects && !b.eligible && !b.checked, "{b:?}");
    assert_eq!(b.reason.as_deref(), Some(MOVES));
    // Marked for the list, which lets it be checked (mitcad#1789).
    assert!(b.joint_motion, "{b:?}");
    // A (grounded), C (no joint) and D (fixed to A) are cut.
    for i in [0, 2, 3] {
        let r = row(rig.components[i].0);
        assert!(
            r.eligible && r.checked && r.reason.is_none() && !r.joint_motion,
            "{r:?}"
        );
    }
    let uid = rig.add(&cut);
    assert_eq!(rig.doc.status(uid), Some(&FeatureStatus::Ok));
    assert_eq!(rig.cut_bodies(), [true, false, true, true]);
}

#[test]
fn a_cut_inside_a_turning_part_cuts_only_its_own_body() {
    let mut rig = rig();
    rig.joints();
    // B's own cut, its tool through all four boxes: only B moves against
    // the others, so it cuts its own body and leaves A, C and D alone.
    rig.doc.activate_component(rig.components[1].0).unwrap();
    let sketch = rig.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = rig
        .doc
        .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let b = rig.path(1);
    let cut = json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
        "extent": {"type": "two_sides", "side1": {"type": "distance", "distance": 80},
                   "side2": {"type": "distance", "distance": 80}},
        "operation": "cut",
        "cut_scope": {"context": {"component": rig.components[1].0, "occurrence": b},
                      "targets": {"type": "automatic"}}});
    let uid = rig.add(&cut);
    assert_eq!(rig.doc.status(uid), Some(&FeatureStatus::Ok));
    assert_eq!(rig.cut_bodies(), [false, true, false, false]);
}

#[test]
fn an_explicit_scope_still_cuts_a_turning_part() {
    let mut rig = rig();
    rig.joints();
    let included: Vec<Value> = (0..4)
        .map(|i| json!({"component": rig.components[i].0, "occurrence": rig.path(i)}))
        .collect();
    let cut = rig.cut(json!({"type": "explicit", "included": included}));
    // No row is marked: an explicit scope leaves nothing out for joints.
    let report = rig.doc.preview_add(&def(cut.clone()), None).unwrap();
    let rows = report.cut_targets.unwrap().rows;
    assert!(
        rows.iter().all(|r| r.eligible && !r.joint_motion),
        "{rows:?}"
    );
    let uid = rig.add(&cut);
    assert_eq!(rig.doc.status(uid), Some(&FeatureStatus::Ok));
    assert_eq!(rig.cut_bodies(), [true; 4]);
}

#[test]
fn a_joint_added_later_takes_its_parts_out_of_an_earlier_cut() {
    let mut rig = rig();
    let uid = rig.add(&rig.cut(json!({"type": "automatic"})));
    assert_eq!(rig.cut_bodies(), [true; 4]);
    // The joints come after the cut in the timeline: they still say how
    // the design moves, and the cut is evaluated again.
    let [revolute, _] = rig.joints();
    assert_eq!(rig.doc.status(uid), Some(&FeatureStatus::Ok));
    assert_eq!(rig.cut_bodies(), [true, false, true, true]);
    // Suppressed, the revolute joint holds nothing: B is cut again.
    rig.doc.set_suppressed(revolute, true).unwrap();
    assert_eq!(rig.cut_bodies(), [true; 4]);
    rig.doc.set_suppressed(revolute, false).unwrap();
    assert_eq!(rig.cut_bodies(), [true, false, true, true]);
    // Without its ground, A turns against B as much as B against it, and
    // the root's cut leaves both alone; D, fixed to A, goes with it.
    rig.doc
        .set_occurrence_grounded(rig.components[0].1, false)
        .unwrap();
    assert_eq!(rig.cut_bodies(), [false, false, true, false]);
}
