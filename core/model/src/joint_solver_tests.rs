// SPDX-License-Identifier: MIT
//! The joint solver in the model (mitcad#55, phase 2) with the mock
//! kernel: loops of joints solved at each point of the timeline, exact
//! degrees of freedom, driven positions (values, parameters), drags,
//! limits, rigid groups that move as one, contradicting joints and
//! sub-assemblies.
//!
//! Links are components without bodies: their joints are on fixed points
//! of their coordinates (a frame along the component's axes).

use std::f64::consts::{FRAC_PI_2, FRAC_PI_3, FRAC_PI_4};

use serde_json::{Value, json};

use crate::datum::Vec3;
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::testing::MockKernel;
use crate::transform::Transform;
use crate::{Document, FeatureStatus};

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

fn shift(x: f64, y: f64, z: f64) -> Transform {
    Transform::translation([x, y, z])
}

fn turn(angle: f64) -> Transform {
    Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], angle).unwrap()
}

fn assert_near(a: Vec3, b: Vec3) {
    assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-8), "{a:?} != {b:?}");
}

fn dist(a: Vec3, b: Vec3) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// A component without bodies, its occurrence at `at` in the root.
fn link(doc: &mut Document<MockKernel>, name: &str, at: Transform) -> OccurrenceUid {
    let (_, o) = doc.create_component(Some(name), at, false).unwrap();
    o
}

/// An origin on a point of an occurrence's component.
fn pin(occurrence: &str, p: Vec3) -> Value {
    json!({"occurrence": occurrence, "geometry": {"point": p}})
}

/// An origin on a point of the root (fixed).
fn ground(p: Vec3) -> Value {
    json!({"geometry": {"point": p}})
}

fn joint(doc: &mut Document<MockKernel>, kind: &str, a: Value, b: Value) -> Value {
    run(
        doc,
        json!({"cmd": "add_joint", "kind": kind, "a": a, "b": b}),
    )
}

fn uid(result: &Value) -> FeatureUid {
    result["uid"].as_str().unwrap().parse().unwrap()
}

/// Where a point of an occurrence's component is in the root.
fn world(doc: &Document<MockKernel>, o: OccurrenceUid, p: Vec3) -> Vec3 {
    doc.placement(o).unwrap().apply_point(p)
}

fn joint_json(doc: &Document<MockKernel>, uid: FeatureUid) -> Value {
    query(doc, json!({"query": "joints"}))["joints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["uid"] == uid.to_string())
        .unwrap()
        .clone()
}

/// Ground pivots at the origin and 100 along x, a crank of 30, a coupler of
/// 100 and a rocker of 60, each placed somewhere else to begin with.
struct FourBar {
    doc: Document<MockKernel>,
    crank: OccurrenceUid,
    coupler: OccurrenceUid,
    rocker: OccurrenceUid,
    joints: Vec<FeatureUid>,
}

fn four_bar() -> FourBar {
    let mut doc = Document::new(MockKernel::default());
    let crank = link(&mut doc, "Crank", shift(0.0, 200.0, 50.0));
    let coupler = link(&mut doc, "Coupler", shift(0.0, -300.0, 0.0));
    let rocker = link(
        &mut doc,
        "Rocker",
        shift(500.0, 0.0, 0.0).after(&turn(FRAC_PI_2)),
    );
    let mut joints = Vec::new();
    for (a, b) in [
        (pin("Crank:1", [0.0; 3]), ground([0.0; 3])),
        (pin("Coupler:1", [0.0; 3]), pin("Crank:1", [30.0, 0.0, 0.0])),
        (
            pin("Rocker:1", [0.0; 3]),
            pin("Coupler:1", [100.0, 0.0, 0.0]),
        ),
        (pin("Rocker:1", [60.0, 0.0, 0.0]), ground([100.0, 0.0, 0.0])),
    ] {
        let result = joint(&mut doc, "revolute", a, b);
        assert_eq!(result["error"], Value::Null, "{result}");
        joints.push(uid(&result));
    }
    FourBar {
        doc,
        crank,
        coupler,
        rocker,
        joints,
    }
}

impl FourBar {
    /// Every joint holds: the pins meet.
    fn check(&self) {
        let w = |o, p| world(&self.doc, o, p);
        assert_near(w(self.crank, [0.0; 3]), [0.0; 3]);
        assert_near(w(self.crank, [30.0, 0.0, 0.0]), w(self.coupler, [0.0; 3]));
        assert_near(w(self.coupler, [100.0, 0.0, 0.0]), w(self.rocker, [0.0; 3]));
        assert_near(w(self.rocker, [60.0, 0.0, 0.0]), [100.0, 0.0, 0.0]);
        for o in [self.crank, self.coupler, self.rocker] {
            assert!(self.doc.placement(o).unwrap().translation[2].abs() < 1e-8);
        }
        for j in &self.joints {
            let joint = joint_json(&self.doc, *j);
            assert_eq!(joint["solved"], true, "{joint}");
        }
    }

    fn crank_pin(&self) -> Vec3 {
        world(&self.doc, self.crank, [30.0, 0.0, 0.0])
    }
}

#[test]
fn a_four_bar_linkage_solves_at_each_timeline_position() {
    let mut fb = four_bar();
    fb.check();
    // The loop's last joint moved the links to close it.
    let closing = joint_json(&fb.doc, fb.joints[3]);
    assert_eq!(closing["state"], "placed", "{closing}");
    // Exact degrees of freedom: one, though the joints take away more
    // than the links have; the closing joint repeats three equations.
    let dof = &query(&fb.doc, json!({"query": "joints"}))["dof"][0];
    assert_eq!(dof["total"], 1, "{dof}");
    assert_eq!(dof["overconstrained"], true);
    assert_eq!(dof["redundant"], json!([fb.joints[3]]));
    assert_eq!(dof["conflicting"], json!([]));
    for unit in dof["units"].as_array().unwrap() {
        assert_eq!(unit["dof"], 0, "{unit}");
    }

    // Before the closing joint: an open chain, each link placed on the one
    // before at the turn it has, nothing iterated.
    run(&mut fb.doc, json!({"cmd": "set_marker", "position": 3}));
    let w = |doc: &Document<MockKernel>, o, p| world(doc, o, p);
    assert_near(w(&fb.doc, fb.crank, [30.0, 0.0, 0.0]), [30.0, 0.0, 0.0]);
    assert_near(w(&fb.doc, fb.coupler, [100.0, 0.0, 0.0]), [130.0, 0.0, 0.0]);
    assert_near(w(&fb.doc, fb.rocker, [60.0, 0.0, 0.0]), [130.0, 60.0, 0.0]);
    let dof = &query(&fb.doc, json!({"query": "joints"}))["dof"][0];
    assert_eq!(dof["total"], 3);
    assert_eq!(dof["overconstrained"], false);
    // Before any joint: where the occurrences were placed.
    run(&mut fb.doc, json!({"cmd": "set_marker", "position": 0}));
    assert_near(
        fb.doc.placement(fb.crank).unwrap().translation,
        [0.0, 200.0, 50.0],
    );
    run(&mut fb.doc, json!({"cmd": "set_marker", "position": 4}));
    fb.check();
    // The occurrences' own placements stay the starting ones.
    let own = fb.doc.assembly().occurrence(fb.crank).unwrap().transform;
    assert_near(own.translation, [0.0, 200.0, 50.0]);
}

#[test]
fn driven_positions_move_the_linkage() {
    let mut fb = four_bar();
    // Driven by a value: the crank goes there and the rest follows.
    let result = run(
        &mut fb.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"rz": "60 deg"}}),
    );
    assert_eq!(result["error"], Value::Null, "{result}");
    assert_eq!(result["parameters"].as_array().unwrap().len(), 1);
    fb.check();
    let c = FRAC_PI_3.cos() * 30.0;
    let s = FRAC_PI_3.sin() * 30.0;
    assert_near(fb.crank_pin(), [c, s, 0.0]);
    let joint = joint_json(&fb.doc, fb.joints[0]);
    assert!((joint["position"]["rz"].as_f64().unwrap() - FRAC_PI_3).abs() < 1e-12);
    assert!((joint["values"]["rz"].as_f64().unwrap() - FRAC_PI_3).abs() < 1e-9);
    // The degrees of freedom are the joints': a driven value does not
    // count.
    assert_eq!(
        query(&fb.doc, json!({"query": "joints"}))["dof"][0]["total"],
        1
    );

    // Driven by a parameter: changing it moves the linkage.
    run(
        &mut fb.doc,
        json!({"cmd": "add_parameter", "name": "crank_angle", "expression": "45 deg"}),
    );
    run(
        &mut fb.doc,
        json!({"cmd": "drive_joint", "joint": fb.joints[0], "values": {"rz": "crank_angle"}}),
    );
    fb.check();
    let r = 30.0 * FRAC_PI_4.cos();
    assert_near(fb.crank_pin(), [r, r, 0.0]);
    run(
        &mut fb.doc,
        json!({"cmd": "set_parameter", "name": "crank_angle", "expression": "100 deg"}),
    );
    fb.check();
    let a = 100.0_f64.to_radians();
    assert_near(fb.crank_pin(), [30.0 * a.cos(), 30.0 * a.sin(), 0.0]);
    // The position is in the definition and survives the project file.
    let text = fb.doc.to_json();
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    assert_near(world(&copy, fb.crank, [30.0, 0.0, 0.0]), fb.crank_pin());

    // Undone: back at 60 degrees.
    run(&mut fb.doc, json!({"cmd": "undo"}));
    run(&mut fb.doc, json!({"cmd": "undo"}));
    assert_near(fb.crank_pin(), [c, s, 0.0]);
    // Taken away: free again, the crank stays where it is.
    let result = run(
        &mut fb.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"rz": null}}),
    );
    assert_eq!(result["error"], Value::Null, "{result}");
    assert_eq!(joint_json(&fb.doc, fb.joints[0])["position"], json!({}));
    fb.check();

    // Two driven joints of the loop contradict each other: the closing
    // one fails, naming the joints, and moves nothing.
    run(
        &mut fb.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"rz": "60 deg"}}),
    );
    let result = run(
        &mut fb.doc,
        json!({"cmd": "drive_joint", "joint": "Joint4", "values": {"rz": 0}}),
    );
    let error = result["error"].as_str().unwrap();
    assert!(error.starts_with("Joint4: over-constrained: "), "{error}");
    assert!(error.contains("Joint1 (at its rz position)"), "{error}");
    assert!(error.contains("Joint4 (at its rz position)"), "{error}");
    let closing = joint_json(&fb.doc, fb.joints[3]);
    assert_eq!(closing["state"], "failed");
    assert!(
        error.ends_with(closing["error"].as_str().unwrap()),
        "{closing}"
    );
    // The joints before it still hold.
    let w = |o, p| world(&fb.doc, o, p);
    assert_near(w(fb.crank, [30.0, 0.0, 0.0]), w(fb.coupler, [0.0; 3]));
    run(&mut fb.doc, json!({"cmd": "undo"}));
    fb.check();

    // Wrong motions and positions outside the limits are refused.
    let error = fb
        .doc
        .command(&json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tz": 3}}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("no free motion tz"), "{error}");
    let error = fb
        .doc
        .command(&json!({"cmd": "drive_joint", "joint": "Crank", "values": {}}).to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("no joint Crank"), "{error}");
}

#[test]
fn positions_stay_within_limits_and_rest_values_hold() {
    let mut doc = Document::new(MockKernel::default());
    let slider = link(&mut doc, "Slider", shift(0.0, 0.0, 25.0));
    // A free slide is kept within its limits.
    let result = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "slider", "a": pin("Slider:1", [0.0; 3]),
               "b": ground([0.0; 3]), "limits": {"tz": {"min": 0, "max": 10}}}),
    );
    let j = uid(&result);
    assert_near(doc.placement(slider).unwrap().translation, [0.0, 0.0, 10.0]);
    assert_eq!(joint_json(&doc, j)["within_limits"], true);
    // Driven outside them: refused.
    let error = doc
        .command(&json!({"cmd": "drive_joint", "joint": j, "values": {"tz": 12}}).to_string())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the position of tz is outside its limits"),
        "{error}"
    );
    // Inside: it goes there; a rest value is where it goes without a
    // position.
    run(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": j, "values": {"tz": 4}}),
    );
    assert_near(doc.placement(slider).unwrap().translation, [0.0, 0.0, 4.0]);
    let mut def = query(&doc, json!({"query": "feature", "uid": j}))["def"].clone();
    def["limits"]["tz"]["rest"] = json!(7);
    def["position"] = json!({});
    run(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": j, "def": def}),
    );
    assert_near(doc.placement(slider).unwrap().translation, [0.0, 0.0, 7.0]);

    // A free turn keeps the turn the occurrence has.
    let wheel = link(&mut doc, "Wheel", shift(40.0, 0.0, 0.0).after(&turn(0.5)));
    let result = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "revolute", "a": pin("Wheel:1", [0.0; 3]),
               "b": ground([0.0, 50.0, 0.0])}),
    );
    let values = &joint_json(&doc, uid(&result))["values"];
    assert!(
        (values["rz"].as_f64().unwrap() - 0.5).abs() < 1e-9,
        "{values}"
    );
    assert_near(doc.placement(wheel).unwrap().translation, [0.0, 50.0, 0.0]);
}

#[test]
fn dragging_moves_occurrences_while_the_joints_hold() {
    let mut fb = four_bar();
    let before = fb.doc.placement(fb.rocker).unwrap();
    let grab = world(&fb.doc, fb.rocker, [0.0; 3]);
    let target = [grab[0] + 10.0, grab[1] - 10.0, grab[2]];
    // The query only tells.
    let preview = query(
        &fb.doc,
        json!({"query": "joint_drag", "occurrence": "Rocker:1", "point": grab, "target": target}),
    );
    assert_eq!(fb.doc.placement(fb.rocker).unwrap(), before);
    let placements = preview["placements"].as_array().unwrap();
    assert_eq!(placements.len(), 3, "{preview}");
    assert!(preview["values"][fb.joints[0].to_string()]["rz"].is_number());
    // The command keeps it: the occurrences' own placements move, the
    // joints still hold, and the crank turned.
    let crank_before = fb.crank_pin();
    let result = run(
        &mut fb.doc,
        json!({"cmd": "drag_occurrence", "occurrence": "Rocker:1", "point": grab,
               "target": target}),
    );
    assert_eq!(result["error"], Value::Null, "{result}");
    fb.check();
    assert!(dist(fb.crank_pin(), crank_before) > 1e-3);
    let rows = &result["placements"][0]["transform"];
    assert_eq!(rows.as_array().unwrap().len(), 3);
    // Undone in one step.
    assert_eq!(
        run(&mut fb.doc, json!({"cmd": "undo"}))["label"],
        "Drag Rocker:1"
    );
    assert_eq!(fb.doc.placement(fb.rocker).unwrap(), before);

    // A driven joint follows the drag: its position is the new value.
    run(
        &mut fb.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"rz": "60 deg"}}),
    );
    let grab = world(&fb.doc, fb.rocker, [0.0; 3]);
    run(
        &mut fb.doc,
        json!({"cmd": "drag_occurrence", "occurrence": "Rocker:1", "point": grab,
               "target": [grab[0] + 5.0, grab[1], grab[2]]}),
    );
    fb.check();
    let joint = joint_json(&fb.doc, fb.joints[0]);
    let position = joint["position"]["rz"].as_f64().unwrap();
    assert!((position - FRAC_PI_3).abs() > 1e-3, "{joint}");
    assert!((joint["values"]["rz"].as_f64().unwrap() - position).abs() < 1e-9);

    // A grounded occurrence does not drag.
    fb.doc.set_occurrence_grounded(fb.crank, true).unwrap();
    let error = fb
        .doc
        .query(
            &json!({"query": "joint_drag", "occurrence": "Crank:1", "target": [0, 0, 0]})
                .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("Crank:1 is grounded"), "{error}");
}

#[test]
fn a_slider_crank_slides_its_piston() {
    let mut doc = Document::new(MockKernel::default());
    let crank = link(&mut doc, "Crank", shift(50.0, 50.0, 0.0));
    link(&mut doc, "Rod", shift(-40.0, 0.0, 9.0));
    let piston = link(&mut doc, "Piston", shift(0.0, -90.0, 3.0));
    for (kind, a, b) in [
        ("revolute", pin("Crank:1", [0.0; 3]), ground([0.0; 3])),
        (
            "revolute",
            pin("Rod:1", [0.0; 3]),
            pin("Crank:1", [20.0, 0.0, 0.0]),
        ),
        (
            "revolute",
            pin("Piston:1", [0.0; 3]),
            pin("Rod:1", [80.0, 0.0, 0.0]),
        ),
    ] {
        joint(&mut doc, kind, a, b);
    }
    let slider = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "x",
               "a": pin("Piston:1", [0.0; 3]), "b": ground([0.0; 3]),
               "limits": {"tx": {"min": 0, "max": 200}}}),
    );
    assert_eq!(slider["error"], Value::Null, "{slider}");
    assert_eq!(
        query(&doc, json!({"query": "joints"}))["dof"][0]["total"],
        1
    );
    for degrees in [0.0, 30.0, 90.0, 150.0, 180.0, 250.0] {
        run(
            &mut doc,
            json!({"cmd": "drive_joint", "joint": "Joint1",
                   "values": {"rz": format!("{degrees} deg")}}),
        );
        let a = f64::to_radians(degrees);
        let x = 20.0 * a.cos() + (80.0_f64.powi(2) - (20.0 * a.sin()).powi(2)).sqrt();
        assert_near(
            world(&doc, crank, [20.0, 0.0, 0.0]),
            [20.0 * a.cos(), 20.0 * a.sin(), 0.0],
        );
        assert_near(world(&doc, piston, [0.0; 3]), [x, 0.0, 0.0]);
        let values = &joint_json(&doc, uid(&slider))["values"];
        assert!(
            (values["tx"].as_f64().unwrap() - x).abs() < 1e-8,
            "{values}"
        );
    }
}

#[test]
fn joints_that_cannot_hold_fail_and_move_nothing() {
    // Ground pivots 300 apart: the links cannot reach.
    let mut doc = Document::new(MockKernel::default());
    link(&mut doc, "Crank", Transform::IDENTITY);
    link(&mut doc, "Coupler", shift(0.0, 50.0, 0.0));
    let rocker = link(&mut doc, "Rocker", shift(0.0, 100.0, 0.0));
    joint(
        &mut doc,
        "revolute",
        pin("Crank:1", [0.0; 3]),
        ground([0.0; 3]),
    );
    joint(
        &mut doc,
        "revolute",
        pin("Coupler:1", [0.0; 3]),
        pin("Crank:1", [30.0, 0.0, 0.0]),
    );
    joint(
        &mut doc,
        "revolute",
        pin("Rocker:1", [0.0; 3]),
        pin("Coupler:1", [100.0, 0.0, 0.0]),
    );
    let before = doc.placement(rocker).unwrap();
    let result = joint(
        &mut doc,
        "revolute",
        pin("Rocker:1", [60.0, 0.0, 0.0]),
        ground([300.0, 0.0, 0.0]),
    );
    let error = result["error"].as_str().unwrap();
    assert!(
        error.contains("cannot all hold") && error.contains("Joint4"),
        "{error}"
    );
    assert!(matches!(
        doc.status(uid(&result)),
        Some(FeatureStatus::Failed(_))
    ));
    assert_eq!(doc.placement(rocker).unwrap(), before);
}

#[test]
fn moves_and_joints_take_rigid_groups_along() {
    let mut doc = Document::new(MockKernel::default());
    let base = link(&mut doc, "Base", Transform::IDENTITY);
    let arm = link(&mut doc, "Arm", shift(100.0, 0.0, 0.0));
    let tool = link(&mut doc, "Tool", shift(120.0, 0.0, 0.0));
    doc.set_occurrence_grounded(base, true).unwrap();
    run(
        &mut doc,
        json!({"cmd": "add_rigid_group", "occurrences": ["Arm:1", "Tool:1"]}),
    );
    // A move of one member moves the group.
    run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "move_occurrence", "occurrences": ["O2"],
               "transform": {"type": "translate_xyz", "x": 0, "y": 10, "z": 0}}}),
    );
    assert_near(doc.placement(arm).unwrap().translation, [100.0, 10.0, 0.0]);
    assert_near(doc.placement(tool).unwrap().translation, [120.0, 10.0, 0.0]);
    // A joint of the arm moves the tool with it.
    let result = joint(
        &mut doc,
        "revolute",
        pin("Arm:1", [0.0; 3]),
        pin("Base:1", [0.0, 0.0, 5.0]),
    );
    assert_eq!(result["state"], "placed", "{result}");
    assert_near(doc.placement(arm).unwrap().translation, [0.0, 0.0, 5.0]);
    assert_near(doc.placement(tool).unwrap().translation, [20.0, 0.0, 5.0]);
    let joint = joint_json(&doc, uid(&result));
    assert_eq!(joint["moved"], json!([arm, tool]));
    let dof = &query(&doc, json!({"query": "joints"}))["dof"][0];
    assert_eq!(dof["total"], 1, "{dof}");
}

#[test]
fn a_rigid_group_with_a_grounded_member_stays() {
    let mut doc = Document::new(MockKernel::default());
    let base = link(&mut doc, "Base", Transform::IDENTITY);
    let bracket = link(&mut doc, "Bracket", shift(50.0, 0.0, 0.0));
    let pin_occurrence = link(&mut doc, "Pin", shift(0.0, 80.0, 0.0));
    doc.set_occurrence_grounded(base, true).unwrap();
    run(
        &mut doc,
        json!({"cmd": "add_rigid_group", "occurrences": ["Base:1", "Bracket:1"]}),
    );
    // A move of the free member would move the grounded one: it fails.
    let moved = run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "move_occurrence", "occurrences": [bracket],
               "transform": {"type": "translate_xyz", "x": 0, "y": 10, "z": 0}}}),
    );
    match doc.status(uid(&moved)) {
        Some(FeatureStatus::Failed(error)) => {
            assert!(error.contains("Base:1 is grounded"), "{error}")
        }
        other => panic!("{other:?}"),
    }
    assert_near(
        doc.placement(bracket).unwrap().translation,
        [50.0, 0.0, 0.0],
    );
    // The group is fixed for joints: a joint onto the bracket places the pin.
    let result = joint(
        &mut doc,
        "revolute",
        pin("Pin:1", [0.0; 3]),
        pin("Bracket:1", [0.0, 0.0, 10.0]),
    );
    assert_eq!(result["state"], "placed", "{result}");
    assert_near(
        doc.placement(pin_occurrence).unwrap().translation,
        [50.0, 0.0, 10.0],
    );
    assert_near(
        doc.placement(bracket).unwrap().translation,
        [50.0, 0.0, 0.0],
    );
    let joint = joint_json(&doc, uid(&result));
    assert_eq!(joint["moved"], json!([pin_occurrence]));
}

#[test]
fn a_sub_assembly_moves_as_a_whole() {
    // A sub-assembly with a pin inside: a joint on the pin's geometry moves
    // the sub-assembly's occurrence, not the pin within it.
    let mut doc = Document::new(MockKernel::default());
    let (sub, sub1) = doc
        .create_component(Some("Sub"), shift(0.0, 100.0, 0.0), true)
        .unwrap();
    let (_, inner) = doc
        .create_component(Some("Pin"), shift(10.0, 0.0, 0.0), false)
        .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let result = joint(
        &mut doc,
        "rigid",
        json!({"occurrence": "Sub:1/Pin:1", "geometry": {"point": [0, 0, 0]}}),
        ground([50.0, 0.0, 0.0]),
    );
    assert_eq!(result["error"], Value::Null, "{result}");
    assert_near(doc.placement(sub1).unwrap().translation, [40.0, 0.0, 0.0]);
    assert_near(doc.placement(inner).unwrap().translation, [10.0, 0.0, 0.0]);
    assert_eq!(doc.assembly().occurrence(inner).unwrap().parent, sub);
}

#[test]
fn as_built_joints_turn_about_their_origin() {
    // A lid on a grounded base, hinged where it is about an axis along y
    // through the lid's origin.
    let mut doc = Document::new(MockKernel::default());
    let base = link(&mut doc, "Base", Transform::IDENTITY);
    let (lid_component, lid) = doc
        .create_component(Some("Lid"), shift(0.0, 0.0, 10.0), true)
        .unwrap();
    let hinge = run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "joint_origin", "geometry": {"point": [0, 0, 0]},
               "frame_override": {"z_axis": [0, 1, 0]}}}),
    );
    assert_eq!(doc.active_component(), lid_component);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_grounded(base, true).unwrap();
    let result = run(
        &mut doc,
        json!({"cmd": "add_as_built_joint", "kind": "revolute", "a": "Lid:1", "b": "Base:1",
               "origin": {"occurrence": "Lid:1", "geometry": hinge["uid"]}}),
    );
    assert_eq!(result["state"], "satisfied", "{result}");
    assert_eq!(joint_json(&doc, uid(&result))["values"], json!({"rz": 0.0}));
    // Driven a quarter turn: the lid turns about the hinge.
    run(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": result["uid"], "values": {"rz": "90 deg"}}),
    );
    assert_near(world(&doc, lid, [10.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
    assert_near(world(&doc, lid, [0.0, 5.0, 0.0]), [0.0, 5.0, 10.0]);
    let values = &joint_json(&doc, uid(&result))["values"];
    assert!(
        (values["rz"].as_f64().unwrap() - FRAC_PI_2).abs() < 1e-9,
        "{values}"
    );
}
