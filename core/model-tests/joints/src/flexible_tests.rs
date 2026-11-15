// SPDX-License-Identifier: MIT
//! Flexible subassemblies (mitcad#1793) with the mock kernel: a Hinge
//! whose Arm turns on a pin at its origin (a revolute joint of the Hinge's
//! own), placed twice in the root. Made flexible, one occurrence's Arm
//! turns in the root for it alone: driven from the root, by a joint of the
//! root to the Arm, by a drag; the other occurrence and the Hinge's design
//! stay as they are.
//!
//! The components have no bodies: the joints are on fixed points of their
//! coordinates (a frame along the component's axes).

use std::f64::consts::{FRAC_PI_2, FRAC_PI_6};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::Vec3;
use mitcad_model::ids::{ComponentUid, OccurrenceUid};
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

struct Hinges {
    doc: Document<MockKernel>,
    /// Hinge:1 at the origin, Hinge:2 200 along x.
    h1: OccurrenceUid,
    h2: OccurrenceUid,
    /// Arm:1, placed in the Hinge.
    arm: OccurrenceUid,
    /// Joint1, the Hinge's.
    joint: String,
}

impl Hinges {
    /// Where an occurrence path puts the Arm's tip in the design.
    fn tip(&self, outer: OccurrenceUid) -> Vec3 {
        self.doc.path_transform(&[outer, self.arm]).apply_point(TIP)
    }

    /// Joint1's values for Hinge:1 alone, from the `joints` query.
    fn flexible_rz(&self) -> f64 {
        let joints = query(&self.doc, json!({"query": "joints"}));
        let entry = joints["flexible"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["occurrence"] == self.h1.to_string())
            .cloned()
            .unwrap_or_else(|| panic!("Hinge:1 is flexible: {joints}"));
        entry["values"][&self.joint]["rz"]
            .as_f64()
            .unwrap_or_else(|| panic!("Joint1's value in Hinge:1: {entry}"))
    }

    /// Joint1's own value, the Hinge's design's.
    fn own_rz(&self) -> f64 {
        let joints = query(&self.doc, json!({"query": "joints"}));
        joints["joints"][0]["values"]["rz"].as_f64().unwrap()
    }
}

/// The Hinge with its Arm and Joint1, placed twice in the root.
fn hinges() -> Hinges {
    let mut doc = Document::new(MockKernel::default());
    let (hinge, h1) = doc
        .create_component(Some("Hinge"), Transform::IDENTITY, true)
        .unwrap();
    let (_, arm) = doc
        .create_component(Some("Arm"), Transform::IDENTITY, false)
        .unwrap();
    doc.activate_component(hinge).unwrap();
    let joint = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "revolute",
               "a": {"occurrence": "Arm:1", "geometry": {"point": [0, 0, 0]}},
               "b": {"geometry": {"point": [0, 0, 0]}}}),
    );
    assert_eq!(joint["solved"], true, "{joint}");
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let h2 = doc
        .copy_occurrence(h1, Some(Transform::translation([200.0, 0.0, 0.0])))
        .unwrap();
    let joint = joint["uid"].as_str().unwrap().to_owned();
    let hinges = Hinges {
        doc,
        h1,
        h2,
        arm,
        joint,
    };
    assert_near(hinges.tip(h1), [50.0, 0.0, 0.0], "Hinge:1's arm");
    assert_near(hinges.tip(h2), [250.0, 0.0, 0.0], "Hinge:2's arm");
    hinges
}

#[test]
fn a_flexible_occurrence_moves_its_own_joints_alone() {
    let mut h = hinges();
    // Driven for Hinge:1 alone, which is not flexible yet: refused.
    let refused = h
        .doc
        .command(
            &json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": "Hinge:1",
                    "values": {"rz": "90 deg"}})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(refused.contains("is not flexible"), "{refused}");

    run(
        &mut h.doc,
        json!({"cmd": "set_occurrence_flexible", "occurrence": "Hinge:1"}),
    );
    assert_eq!(h.doc.undo_label(), Some("Make Hinge:1 Flexible"));
    let tree = query(&h.doc, json!({"query": "components"}));
    let first = &tree["occurrences"][0];
    assert_eq!(first["flexible"], true, "{first}");
    assert_eq!(tree["occurrences"][1]["flexible"], Value::Null);

    // The inner joint driven from the parent: only Hinge:1's arm turns.
    run(
        &mut h.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": "Hinge:1",
               "values": {"rz": "90 deg"}}),
    );
    assert_eq!(h.doc.undo_label(), Some("Drive Joint1 in Hinge:1"));
    assert_near(h.tip(h.h1), [0.0, 50.0, 0.0], "Hinge:1's arm turned");
    assert_near(h.tip(h.h2), [250.0, 0.0, 0.0], "Hinge:2's arm stayed");
    assert!((h.flexible_rz() - FRAC_PI_2).abs() < 1e-9);
    // The Hinge's design did not change: its joint, the Arm's placement.
    assert!(h.own_rz().abs() < 1e-9, "Joint1's own value");
    let placed = h.doc.placement(h.arm).unwrap();
    assert_near(placed.apply_point(TIP), TIP, "the Arm's own placement");
    let def = query(&h.doc, json!({"query": "feature", "uid": h.joint}))["def"].clone();
    assert!(
        def["position"].is_null() || def["position"] == json!({}),
        "Joint1 has no position of its own: {def}"
    );

    // The bodies' world placements follow: instances of the arm's paths.
    let components = query(&h.doc, json!({"query": "components"}));
    let arm_world = &components["occurrences"][0]["children"][0]["world"];
    assert!(
        (arm_world[1][0].as_f64().unwrap() - 1.0).abs() < 1e-9,
        "the arm's world placement in Hinge:1 is turned: {arm_world}"
    );

    // The design file keeps it.
    let text = h.doc.to_json();
    assert!(text.contains("\"flexible\""), "{text}");
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    assert_near(
        copy.path_transform(&[h.h1, h.arm]).apply_point(TIP),
        [0.0, 50.0, 0.0],
        "reopened: Hinge:1's arm",
    );
    assert_near(
        copy.path_transform(&[h.h2, h.arm]).apply_point(TIP),
        [250.0, 0.0, 0.0],
        "reopened: Hinge:2's arm",
    );
    assert_eq!(copy.to_json(), text);

    // The drive taken away: the arm where the Hinge has it.
    run(
        &mut h.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": "Hinge:1",
               "values": {"rz": null}}),
    );
    assert_near(h.tip(h.h1), [50.0, 0.0, 0.0], "Hinge:1's arm back");

    // A joint of the root to the inner arm solves: the arm turns in
    // Hinge:1 (grounded) until its tip is on the root's point.
    h.doc.set_occurrence_grounded(h.h1, true).unwrap();
    let at = [50.0 * FRAC_PI_6.cos(), 50.0 * FRAC_PI_6.sin(), 0.0];
    let ball = run(
        &mut h.doc,
        json!({"cmd": "add_joint", "kind": "ball",
               "a": {"occurrence": "Hinge:1/Arm:1", "geometry": {"point": TIP}},
               "b": {"geometry": {"point": at}}}),
    );
    assert_eq!(ball["solved"], true, "{ball}");
    assert_near(h.tip(h.h1), at, "Hinge:1's arm on the root's point");
    assert_near(
        h.doc.placement(h.h1).unwrap().apply_point([0.0; 3]),
        [0.0; 3],
        "Hinge:1 stayed",
    );
    assert_near(h.tip(h.h2), [250.0, 0.0, 0.0], "Hinge:2's arm stayed");
    assert!((h.flexible_rz() - FRAC_PI_6).abs() < 1e-6);
    assert!(h.own_rz().abs() < 1e-9, "Joint1's own value");

    // Rigid again: Hinge:1's arm where the Hinge has it, and the root's
    // joint moves the whole Hinge:1, which is grounded, so it fails.
    run(
        &mut h.doc,
        json!({"cmd": "set_occurrence_flexible", "occurrence": "Hinge:1", "flexible": false}),
    );
    let joints = query(&h.doc, json!({"query": "joints"}));
    assert_eq!(joints["flexible"], json!([]));
    assert_eq!(joints["joints"][1]["solved"], false, "{joints}");
}

#[test]
fn an_inner_part_of_a_flexible_occurrence_drags_alone() {
    let mut h = hinges();
    h.doc.set_occurrence_flexible(h.h1, true).unwrap();
    h.doc.set_occurrence_grounded(h.h1, true).unwrap();
    // A rigid occurrence's inner part drags in its component (both move):
    // the flexible one's in the root, for it alone.
    let path = format!("{}/{}", h.h1, h.arm);
    let preview = query(
        &h.doc,
        json!({"query": "joint_drag", "occurrence": path, "point": TIP, "target": [0, 50, 0]}),
    );
    let flexed = preview["flexed"].as_array().unwrap();
    assert_eq!(flexed.len(), 1, "{preview}");
    assert_eq!(flexed[0]["flexible"], h.h1.to_string());
    // Where the design shows it: the arm inside Hinge:1 only.
    let design = preview["design"].as_array().unwrap();
    assert_eq!(design.len(), 1, "{preview}");
    assert_eq!(design[0]["path"], path);

    let dragged = run(
        &mut h.doc,
        json!({"cmd": "drag_occurrence", "occurrence": path, "point": TIP,
               "target": [0, 50, 0]}),
    );
    assert_eq!(h.doc.undo_label(), Some("Drag Arm:1"), "{dragged}");
    assert_near(h.tip(h.h1), [0.0, 50.0, 0.0], "Hinge:1's arm dragged");
    assert_near(h.tip(h.h2), [250.0, 0.0, 0.0], "Hinge:2's arm stayed");
    assert!((h.flexible_rz() - FRAC_PI_2).abs() < 1e-6);
    assert!(h.own_rz().abs() < 1e-9, "Joint1's own value");
    // Kept as Hinge:1's own start of the motion.
    let flex = &h.doc.assembly().occurrence(h.h1).unwrap().flexible;
    let start = flex.as_ref().unwrap().start.values().next().unwrap();
    assert!((start[&mitcad_model::joints::Motion::Rz] - FRAC_PI_2).abs() < 1e-6);

    // Undone: back where it was.
    h.doc.undo();
    assert_near(h.tip(h.h1), [50.0, 0.0, 0.0], "undone");
}

#[test]
fn only_a_component_with_joints_can_be_flexible() {
    let mut doc = Document::new(MockKernel::default());
    let (_, plain) = doc
        .create_component(Some("Plain"), Transform::IDENTITY, false)
        .unwrap();
    let refused = doc.set_occurrence_flexible(plain, true).unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("Plain:1 cannot be flexible: Plain has no joints of its own to move"),
        "{refused}"
    );
}

#[test]
fn a_link_of_the_parent_reaches_a_joint_inside_a_flexible_occurrence() {
    let mut h = hinges();
    h.doc.set_occurrence_flexible(h.h1, true).unwrap();
    h.doc.set_occurrence_grounded(h.h1, true).unwrap();
    // A crank of the root on its own pin (Joint2), coupled one to one to
    // Joint1 as Hinge:1 moves it.
    h.doc
        .create_component(
            Some("Crank"),
            Transform::translation([0.0, -100.0, 0.0]),
            false,
        )
        .unwrap();
    run(
        &mut h.doc,
        json!({"cmd": "add_joint", "kind": "revolute",
               "a": {"occurrence": "Crank:1", "geometry": {"point": [0, 0, 0]}},
               "b": {"geometry": {"point": [0, -100, 0]}}}),
    );
    let link = run(
        &mut h.doc,
        json!({"cmd": "add_motion_link",
               "a": {"joint": "Joint2", "motion": "rz"},
               "b": {"joint": "Joint1", "motion": "rz", "occurrence": "Hinge:1"},
               "coupling": {"type": "ratio", "ratio": 1}}),
    );
    assert_eq!(link["solved"], true, "{link}");
    // The crank driven: Hinge:1's arm follows, Hinge:2's stays.
    run(
        &mut h.doc,
        json!({"cmd": "drive_joint", "joint": "Joint2", "values": {"rz": "45 deg"}}),
    );
    let c = std::f64::consts::FRAC_PI_4.cos() * 50.0;
    assert_near(h.tip(h.h1), [c, c, 0.0], "Hinge:1's arm follows the crank");
    assert_near(h.tip(h.h2), [250.0, 0.0, 0.0], "Hinge:2's arm stayed");
    assert!(h.own_rz().abs() < 1e-9, "Joint1's own value");
    let joints = query(&h.doc, json!({"query": "joints"}));
    let link = &joints["links"][0];
    assert_eq!(link["solved"], true, "{link}");
    assert_eq!(link["b"]["occurrence"], h.h1.to_string(), "{link}");

    // A link to a joint inside an occurrence that is not flexible fails.
    h.doc.set_occurrence_flexible(h.h1, false).unwrap();
    let joints = query(&h.doc, json!({"query": "joints"}));
    let error = joints["links"][0]["error"].to_string();
    assert!(
        error.contains("moves in Hinge:1 only when Hinge:1 is flexible"),
        "{joints}"
    );
}
