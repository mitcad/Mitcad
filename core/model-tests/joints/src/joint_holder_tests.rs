// SPDX-License-Identifier: MIT
//! The component a joint's picks belong in (mitcad#1856): the
//! `joint_holder` query finds the lowest component that holds every pick
//! of a joint, an as-built joint, a rigid group or an assembly constraint,
//! and says where the active component stands to it; a joint made there
//! moves a sub-assembly as a whole, its inner joints and rigid group kept.
//!
//! The design is like the reported one: a roller (`Roller:1`, a
//! sub-assembly) with a shaft, a tube and a bearing inside it, the bearing
//! joined to the shaft and the shaft and the tube in a rigid group, both
//! the roller's own features; and a grounded frame (`Frame:1`) beside it
//! in the root. The roller is active, as the reporter had it. Components
//! are links without bodies: origins are fixed points of their coordinates.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::Vec3;
use mitcad_model::ids::{ComponentUid, OccurrenceUid};
use mitcad_model::joints::{HolderRelation, joint_holder};
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

fn shift(x: f64, y: f64, z: f64) -> Transform {
    Transform::translation([x, y, z])
}

fn assert_near(a: Vec3, b: Vec3) {
    assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-6), "{a:?} != {b:?}");
}

/// The component a feature is in; the `feature` query leaves out the
/// root's.
fn feature_component(doc: &Document<MockKernel>, uid: &Value) -> String {
    let feature = query(doc, json!({"query": "feature", "uid": uid}));
    feature["component"].as_str().unwrap_or("C0").to_owned()
}

/// An origin on a point of an occurrence's component.
fn at(occurrence: &str, p: Vec3) -> Value {
    json!({"occurrence": occurrence, "geometry": {"point": p}})
}

struct Dancer {
    doc: Document<MockKernel>,
    roller: ComponentUid,
    roller1: OccurrenceUid,
    shaft1: OccurrenceUid,
    tube1: OccurrenceUid,
    frame1: OccurrenceUid,
}

fn dancer() -> Dancer {
    let mut doc = Document::new(MockKernel::default());
    // The roller at y = 100, active while its parts go in.
    let (roller, roller1) = doc
        .create_component(Some("Roller"), shift(0.0, 100.0, 0.0), true)
        .unwrap();
    let (_, shaft1) = doc
        .create_component(Some("Shaft"), shift(10.0, 0.0, 0.0), false)
        .unwrap();
    let (_, tube1) = doc
        .create_component(Some("Tube"), shift(20.0, 0.0, 0.0), false)
        .unwrap();
    doc.create_component(Some("Bearing"), shift(0.0, 20.0, 0.0), false)
        .unwrap();
    assert_eq!(doc.active_component(), roller);
    // The roller's own joint and rigid group.
    let inner = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": at("Bearing:1", [0.0, 0.0, 0.0]), "b": at("Shaft:1", [0.0, 0.0, 0.0])}),
    );
    assert_eq!(inner["error"], Value::Null, "{inner}");
    let group = run(
        &mut doc,
        json!({"cmd": "add_rigid_group", "occurrences": ["Shaft:1", "Tube:1"]}),
    );
    assert_eq!(group["error"], Value::Null, "{group}");
    // The frame in the root, grounded.
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (_, frame1) = doc
        .create_component(Some("Frame"), Transform::IDENTITY, false)
        .unwrap();
    doc.set_occurrence_grounded(frame1, true).unwrap();
    // The roller active again, as the reporter mounted it.
    doc.activate_component(roller).unwrap();
    Dancer {
        doc,
        roller,
        roller1,
        shaft1,
        tube1,
        frame1,
    }
}

impl Dancer {
    fn holder(&self, occurrences: &[&str], component: Option<&str>) -> Value {
        let mut q = json!({"query": "joint_holder", "occurrences": occurrences});
        if let Some(c) = component {
            q["component"] = json!(c);
        }
        query(&self.doc, q)
    }

    fn in_roller(&self, o: OccurrenceUid) -> String {
        format!("{}/{o}", self.roller1)
    }
}

#[test]
fn the_holder_of_a_roller_part_and_the_frame_is_the_root() {
    let d = dancer();
    let root = d.doc.assembly().name(ComponentUid::ROOT);
    let answer = d.holder(&["Roller:1/Shaft:1", "Frame:1"], None);
    assert_eq!(
        answer["component"],
        ComponentUid::ROOT.to_string(),
        "{answer}"
    );
    assert_eq!(answer["name"], root, "{answer}");
    assert_eq!(answer["path"], "", "{answer}");
    // The roller is active and lies inside it: made in the root, the
    // joint moves the roller as a whole.
    assert_eq!(answer["active"], d.roller.to_string(), "{answer}");
    assert_eq!(answer["active_name"], "Roller", "{answer}");
    assert_eq!(answer["relation"], "inside", "{answer}");
    assert_eq!(answer["whole"], "Roller", "{answer}");
    assert_eq!(
        answer["whole_occurrence"],
        d.roller1.to_string(),
        "{answer}"
    );
    // Each pick's path from the holder and the component it lies in there.
    assert_eq!(
        answer["picks"][0]["path"],
        d.in_roller(d.shaft1),
        "{answer}"
    );
    assert_eq!(answer["picks"][0]["top"], d.roller1.to_string(), "{answer}");
    assert_eq!(answer["picks"][0]["name"], "Roller", "{answer}");
    assert_eq!(answer["picks"][0]["occurrence"], "Roller:1", "{answer}");
    assert_eq!(answer["picks"][1]["path"], d.frame1.to_string(), "{answer}");
    assert_eq!(answer["picks"][1]["name"], "Frame", "{answer}");
    assert_eq!(answer["error"], Value::Null, "{answer}");
    // Uids name the same occurrences.
    let (shaft, frame) = (d.in_roller(d.shaft1), d.frame1.to_string());
    let by_uid = d.holder(&[shaft.as_str(), frame.as_str()], None);
    assert_eq!(by_uid, answer);
}

#[test]
fn the_holder_of_two_roller_parts_is_the_roller() {
    let d = dancer();
    let roller = d.roller.to_string();
    // The roller active: the same component.
    let answer = d.holder(&["Roller:1/Shaft:1", "Roller:1/Tube:1"], None);
    assert_eq!(answer["component"], roller, "{answer}");
    assert_eq!(answer["name"], "Roller", "{answer}");
    assert_eq!(answer["path"], d.roller1.to_string(), "{answer}");
    assert_eq!(answer["relation"], "same", "{answer}");
    assert_eq!(answer["picks"][0]["path"], d.shaft1.to_string(), "{answer}");
    assert_eq!(answer["picks"][0]["name"], "Shaft", "{answer}");
    assert_eq!(answer["picks"][1]["path"], d.tube1.to_string(), "{answer}");
    assert_eq!(answer["whole"], Value::Null, "{answer}");
    // The root: it holds the roller, which a joint made there moves only
    // whole.
    let answer = d.holder(&["Roller:1/Shaft:1", "Roller:1/Tube:1"], Some("C0"));
    assert_eq!(answer["component"], roller, "{answer}");
    assert_eq!(answer["relation"], "above", "{answer}");
    // The frame: neither.
    let answer = d.holder(&["Roller:1/Shaft:1", "Roller:1/Tube:1"], Some("Frame"));
    assert_eq!(answer["relation"], "apart", "{answer}");
    assert_eq!(answer["active_name"], "Frame", "{answer}");
}

#[test]
fn the_holder_takes_every_pick_and_refuses_what_is_no_joint() {
    let d = dancer();
    // A rigid group's three picks.
    let answer = d.holder(&["Roller:1/Shaft:1", "Roller:1/Tube:1", "Frame:1"], None);
    assert_eq!(
        answer["component"],
        ComponentUid::ROOT.to_string(),
        "{answer}"
    );
    assert_eq!(answer["picks"].as_array().unwrap().len(), 3, "{answer}");
    // The roller's own geometry and a part inside it: the roller holds
    // both, one of them its own geometry.
    let answer = d.holder(&["Roller:1", "Roller:1/Tube:1"], Some("C0"));
    assert_eq!(answer["component"], d.roller.to_string(), "{answer}");
    assert_eq!(answer["picks"][0]["path"], "", "{answer}");
    assert_eq!(answer["picks"][0]["top"], Value::Null, "{answer}");
    assert_eq!(answer["picks"][0]["name"], "Roller", "{answer}");
    assert_eq!(answer["error"], Value::Null, "{answer}");
    // Both picks on one part: no joint joins them, in words.
    let answer = d.holder(&["Roller:1/Shaft:1", "Roller:1/Shaft:1"], None);
    let error = answer["error"].as_str().unwrap_or_default();
    assert!(error.starts_with("both picks are on Shaft:1"), "{answer}");
    assert!(!error.contains(&d.shaft1.to_string()), "{answer}");
    // One pick, or a path that leads nowhere, is refused.
    for refused in [
        json!({"query": "joint_holder", "occurrences": ["Frame:1"]}),
        json!({"query": "joint_holder", "occurrences": ["Frame:1", "Roller:1/Nothing:1"]}),
    ] {
        assert!(d.doc.query(&refused.to_string()).is_err(), "{refused}");
    }
}

#[test]
fn joint_holder_compares_the_paths_it_is_given() {
    // The model's function the query calls, on its own.
    let d = dancer();
    let assembly = d.doc.assembly();
    let paths = vec![vec![d.roller1, d.shaft1], vec![d.frame1]];
    let holder = joint_holder(assembly, &paths, d.roller).unwrap();
    assert_eq!(holder.component, ComponentUid::ROOT);
    assert!(holder.path.is_empty());
    assert_eq!(holder.picks, paths);
    assert_eq!(holder.relation, HolderRelation::Inside);
    assert_eq!(holder.whole, Some(d.roller1));
    let holder = joint_holder(assembly, &paths, ComponentUid::ROOT).unwrap();
    assert_eq!(holder.relation, HolderRelation::Same);
    assert_eq!(holder.whole, None);
    assert!(joint_holder(assembly, &paths[..1], d.roller).is_err());
}

#[test]
fn a_joint_made_in_the_holder_moves_the_roller_whole() {
    let mut d = dancer();
    // Made in the active roller, the frame is outside it: refused.
    let mount = json!({"cmd": "add_joint", "kind": "rigid",
                       "a": at("Roller:1/Shaft:1", [0.0, 0.0, 0.0]),
                       "b": at("Frame:1", [50.0, 0.0, 0.0])});
    assert!(d.doc.command(&mount.to_string()).is_err());
    // Made in the root, which holds both, while the roller stays active.
    let mut in_root = mount.clone();
    in_root["component"] = json!("C0");
    let result = run(&mut d.doc, in_root);
    assert_eq!(result["error"], Value::Null, "{result}");
    assert_eq!(d.doc.active_component(), d.roller);
    assert_eq!(feature_component(&d.doc, &result["uid"]), "C0");
    let uid = result["uid"].as_str().unwrap().to_owned();
    // The roller moved as a whole: the shaft's origin on the frame's
    // point, the shaft where it was inside the roller.
    assert_near(
        d.doc.placement(d.roller1).unwrap().translation,
        [40.0, 0.0, 0.0],
    );
    assert_near(
        d.doc.placement(d.shaft1).unwrap().translation,
        [10.0, 0.0, 0.0],
    );
    let joints = query(&d.doc, json!({"query": "joints"}));
    let mount = joints["joints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["uid"] == uid)
        .unwrap();
    assert_eq!(mount["moved"], json!([d.roller1.to_string()]), "{mount}");
    // The roller's own joint and rigid group are kept and hold.
    let inner = joints["joints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["component"] == d.roller.to_string())
        .unwrap();
    assert_eq!(inner["error"], Value::Null, "{inner}");
    assert_ne!(inner["state"], "failed", "{inner}");
    let group = &joints["rigid_groups"][0];
    assert_eq!(group["component"], d.roller.to_string(), "{group}");
    assert_eq!(group["active"], true, "{group}");
}

#[test]
fn as_built_joints_rigid_groups_and_constraints_go_into_the_holder_too() {
    // An as-built joint of the roller and the frame, made in the root.
    let mut d = dancer();
    let result = run(
        &mut d.doc,
        json!({"cmd": "add_as_built_joint", "kind": "rigid", "a": "Roller:1", "b": "Frame:1",
               "component": "C0"}),
    );
    assert_eq!(result["error"], Value::Null, "{result}");
    assert_eq!(feature_component(&d.doc, &result["uid"]), "C0");
    assert_eq!(d.doc.active_component(), d.roller);

    // A rigid group of the roller and the frame, made in the root.
    let mut d = dancer();
    let result = run(
        &mut d.doc,
        json!({"cmd": "add_rigid_group", "occurrences": ["Roller:1", "Frame:1"], "component": "C0"}),
    );
    assert_eq!(result["error"], Value::Null, "{result}");
    let joints = query(&d.doc, json!({"query": "joints"}));
    let group = joints["rigid_groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["component"] == "C0")
        .unwrap();
    assert_eq!(
        group["occurrences"],
        json!([d.roller1.to_string(), d.frame1.to_string()]),
        "{group}"
    );

    // An assembly constraint of a shaft point on the frame's point, made in
    // the root: the roller moves as a whole.
    let mut d = dancer();
    let result = run(
        &mut d.doc,
        json!({"cmd": "add_constraint_set", "component": "C0",
               "rows": [{"type": "align", "a": at("Roller:1/Shaft:1", [0.0, 0.0, 0.0]),
                         "b": at("Frame:1", [50.0, 0.0, 0.0])}]}),
    );
    assert_eq!(result["error"], Value::Null, "{result}");
    assert_eq!(feature_component(&d.doc, &result["uid"]), "C0");
    assert_eq!(d.doc.active_component(), d.roller);
    let roller = d.doc.placement(d.roller1).unwrap();
    let shaft = d.doc.placement(d.shaft1).unwrap();
    assert_near(roller.after(&shaft).apply_point([0.0; 3]), [50.0, 0.0, 0.0]);
    assert_near(shaft.translation, [10.0, 0.0, 0.0]);
}

#[test]
fn a_preview_in_the_holder_shows_the_joint_as_it_will_be_made() {
    // The application previews a new joint with the component OK will add
    // it to (the preview command's `component`, as add_feature's).
    let mut d = dancer();
    let def = json!({"type": "joint", "kind": "rigid",
                     "a": {"occurrence": d.in_roller(d.shaft1), "geometry": {"point": [0.0, 0.0, 0.0]}},
                     "b": {"occurrence": d.frame1.to_string(), "geometry": {"point": [50.0, 0.0, 0.0]}}});
    // In the active roller: the frame is outside it, no preview.
    let in_roller = json!({"cmd": "add_feature", "def": def}).to_string();
    if let Ok(report) = d.doc.preview(&in_roller) {
        let report: Value = serde_json::from_str(&report).unwrap();
        assert_ne!(report["status"], "ok", "{report}");
    }
    // In the root, which holds both: the roller placed whole, nothing
    // committed and the roller still active.
    let in_root = json!({"cmd": "add_feature", "def": def, "component": "C0"}).to_string();
    let report: Value = serde_json::from_str(&d.doc.preview(&in_root).unwrap()).unwrap();
    assert_eq!(report["status"], "ok", "{report}");
    // The preview lists the roller and the occurrences inside it, which
    // move with it; nothing outside the roller moves.
    let placements = report["placements"].as_array().unwrap();
    let roller_path = d.roller1.to_string();
    let inside = format!("{roller_path}/");
    assert!(
        placements.iter().all(|p| {
            let path = p["path"].as_str().unwrap();
            path == roller_path || path.starts_with(&inside)
        }),
        "{report}"
    );
    let roller = placements
        .iter()
        .find(|p| p["path"] == roller_path.as_str())
        .unwrap_or_else(|| panic!("{report}"));
    assert!((roller["transform"][0][3].as_f64().unwrap() - 40.0).abs() < 1e-6);
    assert!(roller["transform"][1][3].as_f64().unwrap().abs() < 1e-6);
    assert_near(
        d.doc.placement(d.roller1).unwrap().translation,
        [0.0, 100.0, 0.0],
    );
    assert_eq!(d.doc.active_component(), d.roller);
    // A component that does not exist is refused.
    let nowhere = json!({"cmd": "add_feature", "def": def, "component": "C99"}).to_string();
    assert!(d.doc.preview(&nowhere).is_err());
}
