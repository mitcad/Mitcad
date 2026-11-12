// SPDX-License-Identifier: MIT
//! Motion links between joints (mitcad#466) with the mock kernel: a rack
//! and pinion, gears with a size from a round face, a screw on one joint,
//! a cam's follower, driving either side, dragging, the degrees of
//! freedom, contact sets stopping the linked motion, study playback, the
//! design file, and refused links.
//!
//! The mock's blocks are 10 x 10 x 5 at their component's origin. The
//! plate (Plate:1) is grounded at the origin; the pin (Pin:1) slides along
//! x on the plate's top (x from `tx` to `tx + 10`); the wall (Wall:1,
//! grounded) stands on it from x = 20 to 30; the knob (Knob:1) and the
//! wheel (Wheel:1) turn about z on the plate's top, about (5, 5).

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use serde_json::{Value, json};

use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::joints::Motion;
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

/// A 10 x 10 x 5 block in the active component, and its sketch.
fn block_in(doc: &mut Document<MockKernel>) -> (BodyUid, FeatureUid) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (BodyUid::new(uid, 0), sketch)
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

struct Rig {
    doc: Document<MockKernel>,
    pin: OccurrenceUid,
    slider: FeatureUid,
    knob: FeatureUid,
    wheel: FeatureUid,
    knob_body: BodyUid,
    knob_sketch: FeatureUid,
    wheel_body: BodyUid,
}

fn rig() -> Rig {
    let mut doc = Document::new(MockKernel::default());
    let part = |doc: &mut Document<MockKernel>, name: &str, at: [f64; 3]| {
        let (_, o) = doc
            .create_component(Some(name), Transform::translation(at), true)
            .unwrap();
        let (body, sketch) = block_in(doc);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        (o, body, sketch)
    };
    let (plate, plate_body, _) = part(&mut doc, "Plate", [0.0; 3]);
    let (pin, pin_body, _) = part(&mut doc, "Pin", [100.0, 0.0, 0.0]);
    let (wall, _, _) = part(&mut doc, "Wall", [20.0, 0.0, 5.0]);
    let (_, knob_body, knob_sketch) = part(&mut doc, "Knob", [0.0, 100.0, 0.0]);
    let (_, wheel_body, _) = part(&mut doc, "Wheel", [0.0, 200.0, 0.0]);
    doc.set_occurrence_grounded(plate, true).unwrap();
    doc.set_occurrence_grounded(wall, true).unwrap();
    let top = json!({"occurrence": "Plate:1",
                     "geometry": {"body": plate_body, "face": face(&doc, plate_body, "end")}});
    let bottom = |doc: &Document<MockKernel>, o: &str, body: BodyUid| json!({"occurrence": o, "geometry": {"body": body, "face": face(doc, body, "start")}});
    let pin_bottom = bottom(&doc, "Pin:1", pin_body);
    let knob_bottom = bottom(&doc, "Knob:1", knob_body);
    let wheel_bottom = bottom(&doc, "Wheel:1", wheel_body);
    let uid = |v: &Value| v["uid"].as_str().unwrap().parse().unwrap();
    let slider = uid(&command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "x", "flip": true,
               "a": pin_bottom, "b": top, "position": {"tx": 0}}),
    ));
    let knob = uid(&command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "revolute", "flip": true,
               "a": knob_bottom, "b": top, "position": {"rz": 0}}),
    ));
    let wheel = uid(&command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "revolute", "flip": true,
               "a": wheel_bottom, "b": top}),
    ));
    Rig {
        doc,
        pin,
        slider,
        knob,
        wheel,
        knob_body,
        knob_sketch,
        wheel_body,
    }
}

impl Rig {
    fn run(&mut self, cmd: Value) -> Value {
        command(&mut self.doc, cmd)
    }

    fn value(&self, joint: FeatureUid, motion: Motion) -> f64 {
        self.doc.joint_value(joint, motion)
    }

    fn pin_x(&self) -> f64 {
        self.doc.placement(self.pin).unwrap().translation[0]
    }

    /// A rack and pinion: the knob (radius 2) drives the pin.
    fn rack(&mut self) -> Value {
        self.run(json!({"cmd": "add_motion_link",
                        "a": {"joint": "Joint2", "motion": "rz"},
                        "b": {"joint": "Joint1", "motion": "tx"},
                        "coupling": {"type": "rack_pinion", "radius": 2}}))
    }
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() < tol
}

#[test]
fn a_rack_follows_its_pinion_and_drives_it_back() {
    let mut rig = rig();
    let added = rig.rack();
    assert_eq!(added["name"], "MotionLink1");
    assert_eq!(added["solved"], true);
    assert_eq!(rig.doc.undo_label(), Some("Add MotionLink1"));
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint2", "values": {"rz": "90 deg"}}));
    assert!(near(rig.value(rig.slider, Motion::Tx), PI, 1e-9));
    assert!(near(rig.pin_x(), PI, 1e-9), "{}", rig.pin_x());

    // Driving the rack turns the pinion: the pinion's position is let go.
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 5}}));
    assert!(near(rig.value(rig.knob, Motion::Rz), 2.5, 1e-9));
    assert!(near(rig.pin_x(), 5.0, 1e-9));
    let joints = query(&rig.doc, json!({"query": "joints"}));
    let link = &joints["links"][0];
    assert_eq!(link["coupling"], "rack_pinion");
    assert_eq!(link["ratio"], 2.0);
    assert_eq!(link["solved"], true);
    assert_eq!(link["a"]["name"], "Joint2");
    assert!(joints["joints"][1]["position"].get("rz").is_none());

    // The pinion and the rack move as one: one freedom less.
    let dof = query(
        &rig.doc,
        json!({"query": "joint_dof", "occurrence": "Pin:1"}),
    );
    assert_eq!(dof["dof"], 0);
    assert_eq!(dof["mobile"], true);
    let all = &joints["dof"][0];
    assert_eq!(all["total"], 2, "{all}");

    // Dragging the pin turns the knob.
    let dragged = query(
        &rig.doc,
        json!({"query": "joint_drag", "occurrence": "Pin:1", "point": [5, 5, 5], "target": [7, 5, 5]}),
    );
    let tx = dragged["values"][rig.slider.to_string()]["tx"]
        .as_f64()
        .unwrap();
    let rz = dragged["values"][rig.knob.to_string()]["rz"]
        .as_f64()
        .unwrap();
    assert!(near(tx, 7.0, 1e-6), "{dragged}");
    assert!(near(rz, 3.5, 1e-6), "{dragged}");
}

#[test]
fn gears_take_a_size_from_a_round_face_and_keep_where_they_are() {
    let mut rig = rig();
    // The wheel turned before the link: the link keeps it there.
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint3", "values": {"rz": 1.0}}));
    assert!(near(rig.value(rig.wheel, Motion::Rz), 1.0, 1e-9));
    // The wheel's end face is round (radius 3) for the mock.
    let end = face(&rig.doc, rig.wheel_body, "end");
    rig.doc.kernel().surfaces.borrow_mut().insert(
        end.clone(),
        SurfaceGeometry::Cylinder {
            origin: [5.0, 5.0, 0.0],
            axis: [0.0, 0.0, 1.0],
            radius: 3.0,
        },
    );
    // What a panel asks before it adds the link: the offset keeping it.
    let offset = query(
        &rig.doc,
        json!({"query": "motion_link_offset",
               "a": {"joint": "Joint2", "motion": "rz"}, "b": {"joint": "Joint3", "motion": "rz"},
               "coupling": {"type": "gear", "ra": 6, "rb": 3}}),
    );
    assert!(
        near(offset["offset"].as_f64().unwrap(), 1.0, 1e-9),
        "{offset}"
    );
    let added = rig.run(json!({"cmd": "add_motion_link",
        "a": {"joint": "Joint2", "motion": "rz"}, "b": {"joint": "Joint3", "motion": "rz"},
        "coupling": {"type": "gear", "ra": 6,
                     "rb": {"occurrence": "Wheel:1", "geometry": {"body": rig.wheel_body, "face": end}}}}));
    assert_eq!(added["solved"], true, "{added}");
    // Nothing moved: the offset is where the wheel is.
    assert!(near(rig.value(rig.wheel, Motion::Rz), 1.0, 1e-9));
    let joints = query(&rig.doc, json!({"query": "joints"}));
    assert_eq!(joints["links"][0]["ratio"], -2.0);
    assert!(near(
        joints["links"][0]["offset"].as_f64().unwrap(),
        1.0,
        1e-9
    ));
    // The knob a quarter turn on: the wheel half a turn back.
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint2", "values": {"rz": "90 deg"}}));
    let wheel = rig.value(rig.wheel, Motion::Rz);
    assert!(
        near(wheel, 1.0 - PI, 1e-9) || near(wheel, 1.0 + PI, 1e-9),
        "{wheel}"
    );

    // An internal gear turns the same way.
    let uid: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
    let mut def = serde_json::to_value(
        rig.doc
            .feature(uid)
            .unwrap()
            .def
            .map_params(&mut |_, id| Ok::<_, ()>(rig.doc.parameters().name(*id)))
            .unwrap(),
    )
    .unwrap();
    def["coupling"]["internal"] = json!(true);
    rig.run(json!({"cmd": "edit_feature", "uid": uid.to_string(), "def": def}));
    assert!(
        near(rig.value(rig.wheel, Motion::Rz), 1.0 + PI - 2.0 * PI, 1e-9)
            || near(rig.value(rig.wheel, Motion::Rz), 1.0 + PI, 1e-9)
    );
}

#[test]
fn a_cam_moves_its_follower_on_its_profile() {
    let mut rig = rig();
    // The knob's square (10 x 10 about its axis) is the cam; the pin's
    // centre is the knife's tip, on a line through the axis along x.
    let added = rig.run(json!({"cmd": "add_motion_link",
        "a": {"joint": "Joint2", "motion": "rz"}, "b": {"joint": "Joint1", "motion": "tx"},
        "coupling": {"type": "cam",
                     "profile": {"occurrence": "Knob:1",
                                 "geometry": {"sketch": rig.knob_sketch, "curves": []}}}}));
    assert_eq!(added["solved"], true, "{added}");
    // At 0 the tip stands on the square's side, 5 from the axis.
    assert!(
        near(rig.value(rig.slider, Motion::Tx), 5.0, 1e-6),
        "{}",
        rig.value(rig.slider, Motion::Tx)
    );
    // At 45° on its corner.
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint2", "values": {"rz": "45 deg"}}));
    let tx = rig.value(rig.slider, Motion::Tx);
    assert!(near(tx, 5.0 * 2f64.sqrt(), 1e-6), "{tx}");
    // Back at a quarter turn: the side again.
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint2", "values": {"rz": FRAC_PI_2}}));
    assert!(near(rig.value(rig.slider, Motion::Tx), 5.0, 1e-6));
    let joints = query(&rig.doc, json!({"query": "joints"}));
    assert_eq!(joints["links"][0]["coupling"], "cam");
    assert_eq!(joints["links"][0]["solved"], true);
    // The follower cannot be driven: it follows the cam.
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 3}}),
    );
    assert!(error.contains("follows the cam"), "{error}");

    // A roller of radius 1 rides a radius further out; a flat face on
    // the corner at 45° too.
    let uid: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
    let edit = |rig: &mut Rig, follower: Value| {
        let mut def = serde_json::to_value(
            rig.doc
                .feature(uid)
                .unwrap()
                .def
                .map_params(&mut |_, id| Ok::<_, ()>(rig.doc.parameters().name(*id)))
                .unwrap(),
        )
        .unwrap();
        def["coupling"]["follower"] = follower;
        rig.run(json!({"cmd": "edit_feature", "uid": uid.to_string(), "def": def}));
    };
    edit(&mut rig, json!({"type": "roller", "radius": 1}));
    assert!(near(rig.value(rig.slider, Motion::Tx), 6.0, 1e-6));
    edit(&mut rig, json!({"type": "flat"}));
    rig.run(json!({"cmd": "drive_joint", "joint": "Joint2", "values": {"rz": FRAC_PI_4}}));
    let tx = rig.value(rig.slider, Motion::Tx);
    assert!(near(tx, 5.0 * 2f64.sqrt(), 1e-6), "{tx}");
    assert_eq!(rig.doc.status(rig.knob), Some(&FeatureStatus::Ok));
    let _ = rig.knob_body;
}

#[test]
fn a_screw_couples_one_joints_turn_and_slide() {
    let mut rig = rig();
    // A cylindrical joint of the wheel: it turns and rises along z.
    let wheel_bottom = json!({"occurrence": "Wheel:1",
        "geometry": {"body": rig.wheel_body, "face": face(&rig.doc, rig.wheel_body, "start")}});
    rig.doc.set_suppressed(rig.wheel, true).unwrap();
    let plate_body = rig
        .doc
        .features()
        .find(|f| f.name == "Extrude1")
        .map(|f| BodyUid::new(f.uid, 0))
        .unwrap();
    let top = json!({"occurrence": "Plate:1",
                     "geometry": {"body": plate_body, "face": face(&rig.doc, plate_body, "end")}});
    let screw = command(
        &mut rig.doc,
        json!({"cmd": "add_joint", "kind": "cylindrical", "flip": true,
               "a": wheel_bottom, "b": top, "position": {"rz": 0}}),
    );
    let name = screw["name"].as_str().unwrap().to_owned();
    let uid: FeatureUid = screw["uid"].as_str().unwrap().parse().unwrap();
    rig.run(json!({"cmd": "add_motion_link",
        "a": {"joint": name, "motion": "rz"}, "b": {"joint": name, "motion": "tz"},
        "coupling": {"type": "screw", "lead": 4}}));
    rig.run(json!({"cmd": "drive_joint", "joint": name, "values": {"rz": "720 deg"}}));
    // The wheel stood 5 below the plate's top when linked (its free slide).
    assert!(
        near(rig.value(uid, Motion::Tz), 3.0, 1e-9),
        "{}",
        rig.value(uid, Motion::Tz)
    );
}

#[test]
fn a_contact_stops_the_whole_linked_motion() {
    let mut rig = rig();
    rig.rack();
    rig.run(json!({"cmd": "add_contact_set", "occurrences": ["Pin:1", "Wall:1"]}));
    // The knob would push the pin 20 along; the wall stops it at 10.
    let driven = rig.run(json!({"cmd": "drive_joint", "joint": "Joint2", "values": {"rz": 10}}));
    assert!(driven.get("contact").is_some(), "{driven}");
    let x = rig.pin_x();
    assert!((x - 10.0).abs() < 0.02, "{x}");
    assert!(near(rig.value(rig.knob, Motion::Rz) * 2.0, x, 1e-6));

    // A study keying the knob stops there too.
    rig.doc.undo();
    rig.run(json!({"cmd": "add_motion_study", "steps": 10, "keys": [
        {"joint": "Joint2", "motion": "rz", "points": [[0, 0], [10, 10]]}]}));
    let played = rig.run(json!({"cmd": "play_study", "study": "Study1", "step": 10}));
    assert!(played["fraction"].as_f64().unwrap() < 1.0, "{played}");
    assert!(rig.pin_x() < 10.02);
}

#[test]
fn links_are_saved_and_refused_when_they_do_not_fit() {
    let mut rig = rig();
    rig.rack();
    let text = rig.doc.to_json();
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    assert_eq!(copy.to_json(), text);
    let link = copy
        .features()
        .find(|f| f.name == "MotionLink1")
        .map(|f| f.uid)
        .unwrap();
    assert!(
        copy.link_result(link).is_some_and(|r| r.holds),
        "{:?} {:?}",
        copy.link_result(link),
        copy.status(link)
    );
    assert_eq!(copy.status(link), Some(&FeatureStatus::Ok));

    // Gears couple two turns; a rack a turn and a slide.
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_motion_link", "a": {"joint": "Joint2", "motion": "rz"},
               "b": {"joint": "Joint1", "motion": "tx"},
               "coupling": {"type": "gear", "ra": 1, "rb": 2}}),
    );
    assert!(error.contains("gears couple two turns"), "{error}");
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_motion_link", "a": {"joint": "Joint2", "motion": "rz"},
               "b": {"joint": "Joint3", "motion": "rz"},
               "coupling": {"type": "rack_pinion", "radius": 1}}),
    );
    assert!(error.contains("couples a turn and a slide"), "{error}");
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_motion_link", "a": {"joint": "Joint2", "motion": "tx"},
               "b": {"joint": "Joint3", "motion": "rz"},
               "coupling": {"type": "ratio", "ratio": 1}}),
    );
    assert!(error.contains("no free motion tx"), "{error}");
    let error = refused(
        &mut rig.doc,
        json!({"cmd": "add_motion_link", "a": {"joint": "Joint2", "motion": "rz"},
               "b": {"joint": "Joint3", "motion": "rz"},
               "coupling": {"type": "ratio", "ratio": 0}}),
    );
    assert!(error.contains("other than zero"), "{error}");

    // Deleting a linked joint takes the link with it.
    let before = rig.doc.features().count();
    let slider = rig.slider;
    rig.doc.delete_feature(slider, true).unwrap();
    assert!(rig.doc.features().all(|f| f.name != "MotionLink1"));
    assert!(rig.doc.features().count() < before);
}

/// Animate Joint's frames without a recompute (mitcad#1701): the rack
/// follows its pinion frame by frame, its own position let go as Drive
/// Joint lets it go, and the document stays as it is.
#[test]
fn animate_frames_take_the_linked_motion_along() {
    let mut rig = rig();
    rig.rack();
    let pin_path = rig.pin.to_string();
    let answer = query(
        &rig.doc,
        json!({"query": "joint_poses", "joint": "Joint2", "motion": "rz",
               "values": [FRAC_PI_4, FRAC_PI_2]}),
    );
    let frames = answer["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 2, "{answer}");
    for (frame, turn) in frames.iter().zip([FRAC_PI_4, FRAC_PI_2]) {
        let rz = frame["values"][rig.knob.to_string()]["rz"]
            .as_f64()
            .unwrap();
        let tx = frame["values"][rig.slider.to_string()]["tx"]
            .as_f64()
            .unwrap();
        assert!(near(rz, turn, 1e-9), "{frame}");
        assert!(near(tx, 2.0 * turn, 1e-6), "{frame}");
        // The pin's placement in the design: x is where the rack put it.
        let pin = frame["placements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["path"] == pin_path)
            .unwrap_or_else(|| panic!("the pin moved: {frame}"));
        let x = pin["transform"][0][3].as_f64().unwrap();
        assert!(near(x, 2.0 * turn, 1e-6), "{frame}");
    }
    // Nothing changed.
    assert!(near(rig.pin_x(), 0.0, 1e-9), "{}", rig.pin_x());
    assert_eq!(rig.doc.undo_label(), Some("Add MotionLink1"));
}
