// SPDX-License-Identifier: MIT
//! Motion studies and contact sets (mitcad#454) with the mock kernel: a
//! study's values along its steps, the commands, playing a study, the
//! design file, and contact sets stopping Drive Joint, dragging, previews
//! and playback where the bodies touch.
//!
//! The mock's blocks are 10 x 10 x 5 at their component's origin, and its
//! overlap of two bodies is that of their placed boxes. The plate (Plate:1)
//! is grounded at the origin; the pin (Pin:1) slides along x on the
//! plate's top (x from `tx` to `tx + 10`, z from 5 to 10); the wall
//! (Wall:1, grounded) stands on it from x = 20 to 30; the knob (Knob:1)
//! turns about z on the plate's top.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::joints::Motion;
use mitcad_model::motion::StudyKey;
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string()).unwrap_err().to_string()
}

/// A 10 x 10 x 5 block in the active component.
fn block_in(doc: &mut Document<MockKernel>) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

struct Rig {
    doc: Document<MockKernel>,
    pin: OccurrenceUid,
    slider: FeatureUid,
    knob_joint: FeatureUid,
}

fn face(doc: &Document<MockKernel>, body: BodyUid, role: &str) -> String {
    doc.body_shape(body)
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == role)
        .unwrap()
        .to_string()
}

fn rig() -> Rig {
    let mut doc = Document::new(MockKernel::default());
    let part = |doc: &mut Document<MockKernel>, name: &str, at: [f64; 3]| {
        let (_, o) = doc
            .create_component(Some(name), Transform::translation(at), true)
            .unwrap();
        let body = block_in(doc);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        (o, body)
    };
    let (plate, plate_body) = part(&mut doc, "Plate", [0.0; 3]);
    let (pin, pin_body) = part(&mut doc, "Pin", [100.0, 0.0, 0.0]);
    let (wall, _) = part(&mut doc, "Wall", [20.0, 0.0, 5.0]);
    let (_, knob_body) = part(&mut doc, "Knob", [0.0, 100.0, 0.0]);
    doc.set_occurrence_grounded(plate, true).unwrap();
    doc.set_occurrence_grounded(wall, true).unwrap();
    let top = json!({"occurrence": "Plate:1",
                     "geometry": {"body": plate_body, "face": face(&doc, plate_body, "end")}});
    let bottom = |doc: &Document<MockKernel>, o: &str, body: BodyUid| json!({"occurrence": o, "geometry": {"body": body, "face": face(doc, body, "start")}});
    let pin_bottom = bottom(&doc, "Pin:1", pin_body);
    let knob_bottom = bottom(&doc, "Knob:1", knob_body);
    let slider = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "x", "flip": true,
               "a": pin_bottom, "b": top, "position": {"tx": 0}}),
    );
    let knob_joint = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "revolute", "flip": true,
               "a": knob_bottom, "b": top, "position": {"rz": 0}}),
    );
    let uid = |v: &Value| v["uid"].as_str().unwrap().parse().unwrap();
    Rig {
        slider: uid(&slider),
        knob_joint: uid(&knob_joint),
        doc,
        pin,
    }
}

impl Rig {
    fn run(&mut self, cmd: Value) -> Value {
        command(&mut self.doc, cmd)
    }

    fn value(&self, joint: FeatureUid, motion: Motion) -> f64 {
        self.doc.joint_value(joint, motion)
    }

    /// Where the pin's left side is.
    fn pin_x(&self) -> f64 {
        self.doc.placement(self.pin).unwrap().translation[0]
    }
}

#[test]
fn keys_interpolate_and_hold() {
    let key = StudyKey {
        joint: FeatureUid(1),
        motion: Motion::Rz,
        points: vec![(0.0, 0.0), (10.0, 1.0), (20.0, -1.0)],
    };
    assert_eq!(key.value_at(-5.0), Some(0.0));
    assert_eq!(key.value_at(5.0), Some(0.5));
    assert_eq!(key.value_at(15.0), Some(0.0));
    assert_eq!(key.value_at(99.0), Some(-1.0));
    let empty = StudyKey {
        points: vec![],
        ..key.clone()
    };
    assert_eq!(empty.value_at(1.0), None);
    let mut key = key;
    key.set_point(10.0, 2.0);
    key.set_point(5.0, 4.0);
    assert_eq!(
        key.points,
        vec![(0.0, 0.0), (5.0, 4.0), (10.0, 2.0), (20.0, -1.0)]
    );
}

#[test]
fn the_rig_slides_and_turns() {
    let mut rig = rig();
    assert!((rig.pin_x()).abs() < 1e-9, "{}", rig.pin_x());
    rig.run(json!({"cmd": "drive_joint", "joint": rig.slider.to_string(), "values": {"tx": 15}}));
    assert!((rig.pin_x() - 15.0).abs() < 1e-9);
    assert!((rig.value(rig.slider, Motion::Tx) - 15.0).abs() < 1e-9);
}

#[test]
fn a_study_plays_through_the_commands_and_keeps_its_keys() {
    let mut rig = rig();
    let added = rig.run(json!({"cmd": "add_motion_study", "steps": 40, "keys": [
        {"joint": rig.slider.to_string(), "motion": "tx", "points": [[0, 0], [20, 8]]},
        {"joint": "Joint2", "motion": "rz", "points": [[0, 0], [40, "90 deg"]]}]}));
    assert_eq!(added["name"], "Study1");
    assert_eq!(rig.doc.undo_label(), Some("Add Study1"));
    let values = query(
        &rig.doc,
        json!({"query": "study_values", "study": "Study1", "step": 10}),
    );
    assert_eq!(values["values"][rig.slider.to_string()]["tx"], 4.0);
    let rz = values["values"][rig.knob_joint.to_string()]["rz"]
        .as_f64()
        .unwrap();
    assert!((rz - std::f64::consts::FRAC_PI_8).abs() < 1e-12, "{rz}");

    // Playing to a step drives both joints there: one undo step.
    let played = rig.run(json!({"cmd": "play_study", "study": "Study1", "step": 30}));
    assert_eq!(played["fraction"], 1.0);
    assert!(played.get("contact").is_none());
    assert!((rig.value(rig.slider, Motion::Tx) - 8.0).abs() < 1e-9);
    assert!(
        (rig.value(rig.knob_joint, Motion::Rz) - 0.75 * std::f64::consts::FRAC_PI_2).abs() < 1e-9
    );
    assert!((rig.pin_x() - 8.0).abs() < 1e-9);
    assert_eq!(rig.doc.undo_label(), Some("Play Study1"));
    rig.doc.undo();
    assert!(rig.pin_x().abs() < 1e-9);

    // Add Key at the current step from where the joint is.
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 5}}));
    let key = rig.run(
        json!({"cmd": "set_study_key", "study": "Study1", "joint": "Joint1",
                             "motion": "tx", "step": 30}),
    );
    assert_eq!(key["value"], 5.0);
    let studies = query(&rig.doc, json!({"query": "motion_studies"}));
    assert_eq!(
        studies[0]["keys"][0]["points"],
        json!([[0.0, 0.0], [20.0, 8.0], [30.0, 5.0]])
    );
    assert_eq!(studies[0]["keys"][0]["joint_name"], "Joint1");
    rig.run(
        json!({"cmd": "remove_study_key", "study": "Study1", "joint": "Joint1",
                   "motion": "tx", "step": 20}),
    );
    let studies = query(&rig.doc, json!({"query": "motion_studies"}));
    assert_eq!(
        studies[0]["keys"][0]["points"],
        json!([[0.0, 0.0], [30.0, 5.0]])
    );

    // What a study refuses.
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_motion_study", "keys": [
        {"joint": "Joint1", "motion": "rz", "points": [[0, 0]]}]}),
    );
    assert!(
        error.contains("Joint1 has no free motion rz (it has tx)"),
        "{error}"
    );
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_motion_study", "name": "Study1"}),
    );
    assert!(error.contains("a motion study Study1 exists"), "{error}");
    let error = refused(&mut rig.doc, json!({"cmd": "add_motion_study", "steps": 0}));
    assert!(error.contains("the steps must be 1"), "{error}");
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "play_study", "study": "Nope", "step": 1}),
    );
    assert!(error.contains("there is no motion study Nope"), "{error}");

    // Renamed, then its joint deleted: the key goes with it.
    rig.run(json!({"cmd": "edit_motion_study", "name": "Study1", "new_name": "Open", "steps": 60}));
    let studies = query(&rig.doc, json!({"query": "motion_studies"}));
    assert_eq!(studies[0]["name"], "Open");
    assert_eq!(studies[0]["steps"], 60);
    rig.doc.delete_feature(rig.knob_joint, false).unwrap();
    let studies = query(&rig.doc, json!({"query": "motion_studies"}));
    assert_eq!(studies[0]["keys"].as_array().unwrap().len(), 1);
    rig.run(json!({"cmd": "delete_motion_study", "name": "Open"}));
    assert_eq!(
        query(&rig.doc, json!({"query": "motion_studies"})),
        json!([])
    );
}

#[test]
fn studies_and_contact_sets_survive_the_design_file() {
    let mut rig = rig();
    let empty = rig.doc.to_json();
    assert!(!empty.contains("motion_studies") && !empty.contains("contact_sets"));
    rig.run(
        json!({"cmd": "add_motion_study", "name": "Open", "steps": 50, "keys": [
        {"joint": "Joint1", "motion": "tx", "points": [[0, 0], [50, 30]]},
        {"joint": "Joint2", "motion": "rz", "points": [[10, 0], [50, 1.5]]}]}),
    );
    rig.run(json!({"cmd": "add_contact_set", "occurrences": ["Pin:1", "Wall:1"]}));
    rig.run(
        json!({"cmd": "add_contact_set", "name": "Spare", "suppressed": true,
                   "occurrences": ["Knob:1", "Plate:1"]}),
    );
    let text = rig.doc.to_json();
    assert!(
        text.contains("\"motion_studies\"") && text.contains("\"contact_sets\""),
        "{text}"
    );
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    for q in ["motion_studies", "contact_sets"] {
        assert_eq!(
            query(&rig.doc, json!({"query": q})),
            query(&copy, json!({"query": q}))
        );
    }
    assert_eq!(copy.to_json(), text);
    let sets = query(&copy, json!({"query": "contact_sets"}));
    assert_eq!(sets[0]["name"], "Contact1");
    assert_eq!(sets[0]["occurrences"][1]["name"], "Wall:1");
    assert_eq!(sets[1]["suppressed"], true);

    // A key of a motion its joint does not have, a set of an occurrence
    // that is not there: the file is refused.
    let bad = text.replace("\"motion\": \"rz\"", "\"motion\": \"tz\"");
    let error = Document::from_json(&bad, MockKernel::default())
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("has no free motion tz"),
        "{error}"
    );
    let mut bad: Value = serde_json::from_str(&text).unwrap();
    bad["contact_sets"][0]["occurrences"][1] = json!("O9");
    let error = Document::from_json(&bad.to_string(), MockKernel::default())
        .err()
        .unwrap();
    assert!(
        error.to_string().contains("occurrence O9 does not exist"),
        "{error}"
    );
}

#[test]
fn a_contact_set_stops_a_driven_joint_where_the_bodies_touch() {
    let mut rig = rig();
    // Without a set the pin goes through the wall.
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 30}}));
    assert!((rig.pin_x() - 30.0).abs() < 1e-9);
    rig.doc.undo();
    let added = rig.run(json!({"cmd": "add_contact_set", "occurrences": ["Pin:1", "Wall:1"]}));
    assert_eq!(added["name"], "Contact1");
    let driven = rig.run(json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 30}}));
    let contact = &driven["contact"];
    assert_eq!(contact["set"], "Contact1", "{driven}");
    let names: Vec<&str> = contact["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["occurrence"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Pin:1", "Wall:1"]);
    assert!(
        contact["message"]
            .as_str()
            .unwrap()
            .starts_with("stopped at contact: Body2 (Pin:1)")
    );
    // The pin's right side (x + 10) at the wall (x = 20), within the
    // touching tolerance.
    let x = rig.pin_x();
    assert!((x - 10.0).abs() < 0.02, "{x}");
    assert!(x <= 10.0 + 0.01 + 1e-9, "{x}");
    assert!((rig.value(rig.slider, Motion::Tx) - x).abs() < 1e-9);
    // The stored position is where it stopped.
    let feature = query(
        &rig.doc,
        json!({"query": "feature", "uid": rig.slider.to_string()}),
    );
    assert!(feature["def"]["position"]["tx"].is_string(), "{feature}");
    assert_eq!(
        query(&rig.doc, json!({"query": "contact_check"}))["overlaps"],
        json!([])
    );

    // Away from the wall it moves freely; back toward it, it stops again.
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": -40}}));
    assert!((rig.pin_x() + 40.0).abs() < 1e-9);
    let driven = rig.run(json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 50}}));
    assert!(driven.get("contact").is_some());
    assert!((rig.pin_x() - 10.0).abs() < 0.02);

    // Suppressed, the set does nothing.
    rig.run(json!({"cmd": "edit_contact_set", "name": "Contact1", "suppressed": true}));
    assert_eq!(rig.doc.undo_label(), Some("Suppress Contact1"));
    let driven = rig.run(json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 50}}));
    assert!(driven.get("contact").is_none());
    assert!((rig.pin_x() - 50.0).abs() < 1e-9);
    // Overlapping where a motion starts, a pair is left out: the pin moves
    // on out of the wall.
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 25}}));
    rig.run(json!({"cmd": "edit_contact_set", "name": "Contact1", "suppressed": false}));
    let overlaps = query(&rig.doc, json!({"query": "contact_check"}));
    assert_eq!(overlaps["overlaps"][0]["set"], "Contact1", "{overlaps}");
    let driven = rig.run(json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 45}}));
    assert!(driven.get("contact").is_none(), "{driven}");
    assert!((rig.pin_x() - 45.0).abs() < 1e-9);
}

#[test]
fn contact_stops_dragging_previews_and_playback() {
    let mut rig = rig();
    rig.run(json!({"cmd": "add_contact_set", "occurrences": ["Pin:1", "Wall:1"]}));
    // A drag far past the wall stops where the pin touches it.
    let dragged = query(
        &rig.doc,
        json!({"query": "joint_drag", "occurrence": "Pin:1", "point": [5, 5, 10],
               "target": [45, 5, 10]}),
    );
    assert_eq!(dragged["contact"]["set"], "Contact1", "{dragged}");
    let x = dragged["placements"][0]["transform"][0][3]
        .as_f64()
        .unwrap();
    assert!((x - 10.0).abs() < 0.05, "{x}");
    rig.run(
        json!({"cmd": "drag_occurrence", "occurrence": "Pin:1", "point": [5, 5, 10],
                   "target": [45, 5, 10]}),
    );
    assert!((rig.pin_x() - 10.0).abs() < 0.05, "{}", rig.pin_x());
    rig.doc.undo();

    // Animate Joint's preview, from the last frame's value.
    let preview: Value = serde_json::from_str(
        &rig.doc
            .preview(
                &json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 12},
                        "from": {"tx": 8}})
                .to_string(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(preview["contact"]["set"], "Contact1", "{preview}");
    let fraction = preview["fraction"].as_f64().unwrap();
    assert!((8.0 + 4.0 * fraction - 10.0).abs() < 0.02, "{fraction}");
    assert_eq!(preview["placements"][0]["path"], "O2");
    assert!(rig.pin_x().abs() < 1e-9, "a preview changes nothing");

    // A study's playback: frame by frame along the study.
    rig.run(json!({"cmd": "add_motion_study", "steps": 10, "keys": [
        {"joint": "Joint1", "motion": "tx", "points": [[0, 0], [10, 30]]}]}));
    let frame = |doc: &mut Document<MockKernel>, from: f64, step: f64| -> Value {
        serde_json::from_str(
            &doc.preview(
                &json!({"cmd": "play_study", "study": "Study1", "step": step, "from": from})
                    .to_string(),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let free = frame(&mut rig.doc, 2.0, 3.0);
    assert!(free.get("contact").is_none(), "{free}");
    assert_eq!(free["values"][rig.slider.to_string()]["tx"], 9.0);
    let stopped = frame(&mut rig.doc, 3.0, 4.0);
    assert_eq!(
        stopped["contact"]["bodies"][1]["occurrence"], "Wall:1",
        "{stopped}"
    );
    let tx = stopped["values"][rig.slider.to_string()]["tx"]
        .as_f64()
        .unwrap();
    assert!((tx - 10.0).abs() < 0.02, "{tx}");
    // Played as a command, it stops there and keeps it.
    let played = rig.run(json!({"cmd": "play_study", "study": "Study1", "step": 10}));
    assert!(played["contact"].is_object(), "{played}");
    assert!((rig.pin_x() - 10.0).abs() < 0.02);
}

#[test]
fn contact_sets_follow_their_occurrences() {
    let mut rig = rig();
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_contact_set", "occurrences": ["Pin:1"]}),
    );
    assert!(error.contains("two occurrences or more"), "{error}");
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_contact_set",
                                             "occurrences": ["Pin:1", "Pin:1"]}),
    );
    assert!(error.contains("listed more than once"), "{error}");
    rig.run(json!({"cmd": "add_contact_set", "name": "Stops",
                   "occurrences": ["Pin:1", "O3", "Knob:1"]}));
    rig.run(json!({"cmd": "edit_contact_set", "name": "Stops", "new_name": "Stop"}));
    let sets = query(&rig.doc, json!({"query": "contact_sets"}));
    assert_eq!(sets[0]["name"], "Stop");
    assert_eq!(sets[0]["occurrences"][1]["path"], "O3");
    // A deleted occurrence leaves the set.
    command(
        &mut rig.doc,
        json!({"cmd": "delete_occurrence", "occurrence": "Knob:1"}),
    );
    let sets = query(&rig.doc, json!({"query": "contact_sets"}));
    assert_eq!(
        sets[0]["occurrences"].as_array().unwrap().len(),
        2,
        "{sets}"
    );
    assert!(matches!(
        rig.doc.status(rig.slider),
        Some(FeatureStatus::Ok)
    ));
    rig.run(json!({"cmd": "delete_contact_set", "name": "Stop"}));
    assert_eq!(query(&rig.doc, json!({"query": "contact_sets"})), json!([]));
}
