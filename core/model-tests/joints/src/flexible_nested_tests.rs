// SPDX-License-Identifier: MIT
//! Flexible subassemblies, the follow-ups of mitcad#1805, with the mock
//! kernel: flexible occurrences nested in each other, a joint inside a
//! flexible occurrence animated for it alone, its own joint values carried
//! over by copies, contact sets stopping a joint driven inside it, a
//! sketch on a part inside it seeing the part where it has it, and a
//! motion link of a flexible occurrence's component that couples a joint
//! inside a flexible occurrence of its own.
//!
//! A Frame holds a Hinge (Hinge:1, grounded in the Frame and flexible
//! there) whose Arm turns on a pin at the Hinge's origin (Joint1, the
//! Hinge's own), and a Crank turning on its pin (Joint2, the Frame's own).
//! The Frame is placed twice in the root: Frame:1 at the origin, Frame:2
//! 200 along x. The components have no bodies there: the joints are on
//! fixed points of their coordinates.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::Vec3;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::joints::Motion;
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn run(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn assert_near(a: Vec3, b: Vec3, what: &str) {
    assert!(
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6),
        "{what}: {a:?} != {b:?}"
    );
}

/// The Arm's tip in its coordinates.
const TIP: Vec3 = [50.0, 0.0, 0.0];

struct Frames {
    doc: Document<MockKernel>,
    /// Frame:1 at the origin, Frame:2 200 along x.
    f1: OccurrenceUid,
    f2: OccurrenceUid,
    /// Hinge:1, placed in the Frame.
    hinge: OccurrenceUid,
    /// Arm:1, placed in the Hinge.
    arm: OccurrenceUid,
    /// Joint1 (the Hinge's) and Joint2 (the Frame's).
    joint1: String,
    joint2: String,
}

impl Frames {
    /// Where an occurrence of the Frame puts the Arm's tip in the design.
    fn tip(&self, frame: OccurrenceUid) -> Vec3 {
        self.doc
            .path_transform(&[frame, self.hinge, self.arm])
            .apply_point(TIP)
    }

    /// Frame:1 made flexible and grounded in the root.
    fn flex_first(&mut self) {
        self.doc.set_occurrence_flexible(self.f1, true).unwrap();
        self.doc.set_occurrence_grounded(self.f1, true).unwrap();
    }

    /// A joint's values for Frame:1 alone, from the `joints` query.
    fn flexible_rz(&self, joint: &str) -> f64 {
        let joints = query(&self.doc, json!({"query": "joints"}));
        let entry = joints["flexible"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["occurrence"] == self.f1.to_string())
            .cloned()
            .unwrap_or_else(|| panic!("Frame:1 is flexible: {joints}"));
        entry["values"][joint]["rz"]
            .as_f64()
            .unwrap_or_else(|| panic!("{joint}'s value in Frame:1: {entry}"))
    }
}

/// The Frame with its flexible Hinge and its Crank, placed twice in the
/// root; with `link`, the Frame's motion link turning the Hinge's Arm one
/// to one with the Crank, as Hinge:1 moves it.
fn frames(link: bool) -> Frames {
    let mut doc = Document::new(MockKernel::default());
    let (frame, f1) = doc
        .create_component(Some("Frame"), Transform::IDENTITY, true)
        .unwrap();
    let (_, hinge) = doc
        .create_component(Some("Hinge"), Transform::IDENTITY, true)
        .unwrap();
    let (_, arm) = doc
        .create_component(Some("Arm"), Transform::IDENTITY, false)
        .unwrap();
    let joint1 = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "revolute",
               "a": {"occurrence": "Arm:1", "geometry": {"point": [0, 0, 0]}},
               "b": {"geometry": {"point": [0, 0, 0]}}}),
    );
    assert_eq!(joint1["solved"], true, "{joint1}");
    doc.activate_component(frame).unwrap();
    doc.create_component(
        Some("Crank"),
        Transform::translation([0.0, -100.0, 0.0]),
        false,
    )
    .unwrap();
    let joint2 = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "revolute",
               "a": {"occurrence": "Crank:1", "geometry": {"point": [0, 0, 0]}},
               "b": {"geometry": {"point": [0, -100, 0]}}}),
    );
    assert_eq!(joint2["solved"], true, "{joint2}");
    doc.set_occurrence_grounded(hinge, true).unwrap();
    doc.set_occurrence_flexible(hinge, true).unwrap();
    if link {
        let added = run(
            &mut doc,
            json!({"cmd": "add_motion_link",
                   "a": {"joint": "Joint2", "motion": "rz"},
                   "b": {"joint": "Joint1", "motion": "rz", "occurrence": "Hinge:1"},
                   "coupling": {"type": "ratio", "ratio": 1}}),
        );
        assert_eq!(added["solved"], true, "{added}");
    }
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let f2 = doc
        .copy_occurrence(f1, Some(Transform::translation([200.0, 0.0, 0.0])))
        .unwrap();
    let uid = |v: &Value| v["uid"].as_str().unwrap().to_owned();
    let frames = Frames {
        doc,
        f1,
        f2,
        hinge,
        arm,
        joint1: uid(&joint1),
        joint2: uid(&joint2),
    };
    assert_near(frames.tip(f1), [50.0, 0.0, 0.0], "Frame:1's arm");
    assert_near(frames.tip(f2), [250.0, 0.0, 0.0], "Frame:2's arm");
    frames
}

#[test]
fn flexible_occurrences_nest() {
    let mut f = frames(false);
    f.flex_first();
    // Joint1 moves inside Frame:1 through Hinge:1, both flexible: driven
    // for Frame:1 alone.
    run(
        &mut f.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": "Frame:1",
               "values": {"rz": "90 deg"}}),
    );
    assert_eq!(f.doc.undo_label(), Some("Drive Joint1 in Frame:1"));
    assert_near(f.tip(f.f1), [0.0, 50.0, 0.0], "Frame:1's arm turned");
    assert_near(f.tip(f.f2), [250.0, 0.0, 0.0], "Frame:2's arm stayed");
    assert!((f.flexible_rz(&f.joint1) - FRAC_PI_2).abs() < 1e-9);
    // The Frame's design: the arm where Hinge:1 has it in the Frame.
    assert_near(
        f.doc.path_transform(&[f.hinge, f.arm]).apply_point(TIP),
        TIP,
        "the Frame's own",
    );
    // The components query shows the arm turned inside Frame:1 alone.
    let components = query(&f.doc, json!({"query": "components"}));
    let tree = components.to_string();
    assert!(tree.contains("\"flexible\":true"), "{tree}");

    // Taken away again; then the arm dragged inside Frame:1 and Hinge:1:
    // dragged in the root, kept as Frame:1's own value.
    run(
        &mut f.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": "Frame:1",
               "values": {"rz": null}}),
    );
    assert_near(f.tip(f.f1), [50.0, 0.0, 0.0], "Frame:1's arm back");
    let path = format!("{}/{}/{}", f.f1, f.hinge, f.arm);
    let preview = query(
        &f.doc,
        json!({"query": "joint_drag", "occurrence": path, "point": TIP, "target": [0, 50, 0]}),
    );
    let flexed = preview["flexed"].as_array().unwrap();
    assert_eq!(flexed.len(), 1, "{preview}");
    assert_eq!(flexed[0]["flexible"], f.hinge.to_string(), "{preview}");
    assert_eq!(
        flexed[0]["within"],
        json!([f.f1.to_string(), f.hinge.to_string()]),
        "{preview}"
    );
    run(
        &mut f.doc,
        json!({"cmd": "drag_occurrence", "occurrence": path, "point": TIP,
               "target": [0, 50, 0]}),
    );
    assert_near(f.tip(f.f1), [0.0, 50.0, 0.0], "Frame:1's arm dragged");
    assert_near(f.tip(f.f2), [250.0, 0.0, 0.0], "Frame:2's arm stayed");
    let flex = f.doc.assembly().occurrence(f.f1).unwrap().flexible.clone();
    let start = flex.unwrap().start;
    let kept = start
        .values()
        .find_map(|m| m.get(&Motion::Rz).copied())
        .unwrap();
    assert!((kept - FRAC_PI_2).abs() < 1e-6, "{kept}");

    // Frame:1 rigid again: its arm where the Frame has it.
    f.doc.set_occurrence_flexible(f.f1, false).unwrap();
    assert_near(f.tip(f.f1), [50.0, 0.0, 0.0], "Frame:1 rigid again");
}

#[test]
fn a_joint_inside_a_flexible_occurrence_animates_for_it_alone() {
    let mut f = frames(false);
    f.flex_first();
    let poses = query(
        &f.doc,
        json!({"query": "joint_poses", "joint": "Joint1", "occurrence": "Frame:1",
               "motion": "rz", "values": [FRAC_PI_2]}),
    );
    let frames = poses["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 1, "{poses}");
    let placements = frames[0]["placements"].as_array().unwrap();
    let path = format!("{}/{}/{}", f.f1, f.hinge, f.arm);
    assert_eq!(placements.len(), 1, "{poses}");
    assert_eq!(placements[0]["path"], path, "{poses}");
    // The arm turned a quarter: its x axis along the design's y.
    let m = &placements[0]["transform"];
    assert!((m[1][0].as_f64().unwrap() - 1.0).abs() < 1e-9, "{m}");
    let rz = frames[0]["values"][&f.joint1]["rz"].as_f64().unwrap();
    assert!((rz - FRAC_PI_2).abs() < 1e-9, "{poses}");
    // Nothing changed.
    assert_near(f.tip(f.f1), [50.0, 0.0, 0.0], "Frame:1's arm");

    // Only inside a flexible occurrence.
    let refused = f
        .doc
        .query(
            &json!({"query": "joint_poses", "joint": "Joint1", "occurrence": "Frame:2",
                    "motion": "rz", "values": [1.0]})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("Frame:2 is not flexible"), "{refused}");
}

#[test]
fn copies_keep_a_flexible_occurrence_s_own_values() {
    let mut f = frames(false);
    f.flex_first();
    run(
        &mut f.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": "Frame:1",
               "values": {"rz": "90 deg"}}),
    );
    // Copy and paste: the same component, the same joints and values.
    let copy = f
        .doc
        .copy_occurrence(f.f1, Some(Transform::translation([0.0, 300.0, 0.0])))
        .unwrap();
    assert!(f.doc.is_flexible(copy));
    assert_near(f.tip(copy), [0.0, 350.0, 0.0], "the copy's arm turned");

    // Paste New: a copy of the Frame with copies of its joints, and of the
    // Hinge in it, which stays flexible there; the values follow the
    // copied joints.
    let (made, pasted) = f
        .doc
        .paste_new(f.f1, Some(Transform::translation([0.0, 600.0, 0.0])))
        .unwrap();
    let a = f.doc.assembly();
    let pasted_flex = a.occurrence(pasted).unwrap().flexible.clone().unwrap();
    let inner = a
        .children(made)
        .find(|o| a.children(o.component).next().is_some())
        .cloned()
        .unwrap();
    assert!(inner.flexible.is_some(), "the copied Hinge stays flexible");
    let arm = a.children(inner.component).next().unwrap().uid;
    let original = a.occurrence(f.f1).unwrap().flexible.clone().unwrap();
    assert_eq!(pasted_flex.position.len(), 1, "{pasted_flex:?}");
    assert_ne!(
        pasted_flex.position.keys().next(),
        original.position.keys().next(),
        "the copied joint's value"
    );
    assert_near(
        f.doc
            .path_transform(&[pasted, inner.uid, arm])
            .apply_point(TIP),
        [0.0, 650.0, 0.0],
        "the pasted copy's arm turned",
    );
    assert_near(f.tip(f.f2), [250.0, 0.0, 0.0], "Frame:2's arm stayed");
}

#[test]
fn a_link_naming_a_flexible_occurrence_comes_along_into_the_parent() {
    let mut f = frames(true);
    f.flex_first();
    // The Frame's Crank driven for Frame:1 alone: the Frame's link turns
    // its Hinge:1's arm with it there, through both flexible occurrences.
    run(
        &mut f.doc,
        json!({"cmd": "drive_joint", "joint": "Joint2", "occurrence": "Frame:1",
               "values": {"rz": "45 deg"}}),
    );
    let c = FRAC_PI_4.cos() * 50.0;
    assert_near(f.tip(f.f1), [c, c, 0.0], "Frame:1's arm follows its crank");
    assert_near(f.tip(f.f2), [250.0, 0.0, 0.0], "Frame:2's arm stayed");
    assert!((f.flexible_rz(&f.joint2) - FRAC_PI_4).abs() < 1e-9);
    assert!((f.flexible_rz(&f.joint1) - FRAC_PI_4).abs() < 1e-6);
    // The link itself holds where the Frame has it.
    let joints = query(&f.doc, json!({"query": "joints"}));
    let link = &joints["links"][0];
    assert_eq!(link["solved"], true, "{link}");
}

// Contact sets and linked geometry: blocks of the mock kernel are 10 x 10
// x 5 at their component's origin, and its overlap of two bodies is that
// of their placed boxes.

/// A 10 x 10 x 5 block in the active component, and its top face.
fn block_in(doc: &mut Document<MockKernel>) -> (BodyUid, String) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (BodyUid::new(uid, 0), format!("{uid}:end({region})"))
}

#[test]
fn a_sketch_on_a_part_inside_flexible_occurrences_sees_it_where_they_have_it() {
    let mut f = frames(false);
    f.flex_first();
    // A block on the Arm, and a sketch of the root on its top face seen
    // through Frame:1 and Hinge:1.
    let arm_component = f.doc.assembly().occurrence(f.arm).unwrap().component;
    f.doc.activate_component(arm_component).unwrap();
    let (body, face) = block_in(&mut f.doc);
    f.doc.activate_component(ComponentUid::ROOT).unwrap();
    mitcad_model::test_support::allow_attachments(&mut f.doc);
    let path = format!("{}/{}/{}", f.f1, f.hinge, f.arm);
    let sketch = run(
        &mut f.doc,
        json!({"cmd": "sketch.create", "plane": {"face": face, "body": body},
               "occurrence": path}),
    )["uid"]
        .clone();
    let origin = |doc: &Document<MockKernel>| -> Vec3 {
        let s = query(doc, json!({"query": "sketch", "uid": sketch}));
        let o = s["frame"]["origin"]
            .as_array()
            .unwrap_or_else(|| panic!("no frame: {s}"));
        std::array::from_fn(|i| o[i].as_f64().unwrap())
    };
    let before = origin(&f.doc);
    // Joint1 driven for Frame:1: the sketch turns with the arm about the
    // pin, the design's z axis.
    run(
        &mut f.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": "Frame:1",
               "values": {"rz": "90 deg"}}),
    );
    let after = origin(&f.doc);
    assert_near(
        after,
        [-before[1], before[0], before[2]],
        "the sketch on the arm turned with it",
    );
}

#[test]
fn a_contact_set_stops_a_joint_driven_inside_a_flexible_occurrence() {
    // A Rail whose Pin slides along x (Joint1, the Rail's own) toward its
    // Wall (grounded, x from 20 to 30), placed twice in the root.
    let mut doc = Document::new(MockKernel::default());
    let (rail, r1) = doc
        .create_component(Some("Rail"), Transform::IDENTITY, true)
        .unwrap();
    let (_, pin) = doc
        .create_component(Some("Pin"), Transform::translation([-40.0, 0.0, 0.0]), true)
        .unwrap();
    block_in(&mut doc);
    doc.activate_component(rail).unwrap();
    let (_, wall) = doc
        .create_component(Some("Wall"), Transform::translation([20.0, 0.0, 0.0]), true)
        .unwrap();
    block_in(&mut doc);
    doc.activate_component(rail).unwrap();
    doc.set_occurrence_grounded(wall, true).unwrap();
    let slider = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "x",
               "a": {"occurrence": "Pin:1", "geometry": {"point": [0, 0, 0]}},
               "b": {"geometry": {"point": [0, 0, 0]}}}),
    );
    assert_eq!(slider["solved"], true, "{slider}");
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let r2 = doc
        .copy_occurrence(r1, Some(Transform::translation([0.0, 100.0, 0.0])))
        .unwrap();
    doc.set_occurrence_flexible(r1, true).unwrap();
    doc.set_occurrence_grounded(r1, true).unwrap();
    let pin_x =
        |doc: &Document<MockKernel>, r: OccurrenceUid| doc.path_transform(&[r, pin]).translation[0];
    assert!((pin_x(&doc, r1) + 40.0).abs() < 1e-9);

    // Without a contact set the pin goes through the wall.
    run(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": "Rail:1",
               "values": {"tx": 30}}),
    );
    assert!((pin_x(&doc, r1) - 30.0).abs() < 1e-9);
    doc.undo();
    let pin_path = format!("{r1}/{pin}");
    let wall_path = format!("{r1}/{wall}");
    run(
        &mut doc,
        json!({"cmd": "add_contact_set", "occurrences": [pin_path, wall_path]}),
    );
    // With one, driven inside Rail:1 it stops where the pin touches the
    // wall, as a drag does: the pin's right side (x + 10) at the wall.
    let driven = run(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": "Rail:1",
               "values": {"tx": 30}}),
    );
    assert_eq!(driven["contact"]["set"], "Contact1", "{driven}");
    assert!(driven["warnings"].as_array().is_some_and(|w| !w.is_empty()));
    let x = pin_x(&doc, r1);
    assert!((x - 10.0).abs() < 0.02, "{x}");
    assert!(x <= 10.0 + 0.01 + 1e-9, "{x}");
    // Kept as Rail:1's own position, where it stopped.
    let flex = doc.assembly().occurrence(r1).unwrap().flexible.clone();
    let position = flex.unwrap().position;
    let kept = position
        .values()
        .find_map(|m| m.get(&Motion::Tx).copied())
        .unwrap();
    assert!((kept - x).abs() < 1e-6, "{kept} {x}");
    // Rail:2's pin stayed.
    assert!((pin_x(&doc, r2) + 40.0).abs() < 1e-9);
    // Away from the wall it moves freely.
    let free = run(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": "Rail:1",
               "values": {"tx": -60}}),
    );
    assert!(free.get("contact").is_none(), "{free}");
    assert!((pin_x(&doc, r1) + 60.0).abs() < 1e-9);
}
