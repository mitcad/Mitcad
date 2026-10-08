// SPDX-License-Identifier: MIT
//! Joints between occurrences (mitcad#55) with the mock kernel: the kinds'
//! free motions, joint origins on each kind of geometry, placements a
//! joint settles without a solver, limits as values, as-built joints,
//! rigid groups, degrees of freedom, suppress, edit, delete, undo, copies
//! and the project file.
//!
//! The mock's blocks are 10 x 10 x 5 at their component's origin: the
//! planar end face's middle is (5, 5, 5) with the normal +Z, the start
//! face's (5, 5, 0) with -Z.

use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_2, PI};

use serde_json::{Value, json};

use crate::datum::{CurveGeometry, Datum, SurfaceGeometry, Vec3};
use crate::document_tests::{extrude, num};
use crate::features::SketchPlane;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::{JointKind, Motion, SlideAxis, motion_transform, motion_values};
use crate::testing::MockKernel;
use crate::topo::{EdgeName, VertexName};
use crate::transform::Transform;
use crate::{Document, FeatureStatus};

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

fn shift(x: f64, y: f64, z: f64) -> Transform {
    Transform::translation([x, y, z])
}

fn near(a: Vec3, b: Vec3) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

fn assert_near(a: Vec3, b: Vec3) {
    assert!(near(a, b), "{a:?} != {b:?}");
}

fn json_vec(value: &Value) -> Vec3 {
    std::array::from_fn(|i| value[i].as_f64().unwrap())
}

/// A 10 x 10 x 5 block in the active component: its sketch and body.
fn block_in(doc: &mut Document<MockKernel>) -> (FeatureUid, BodyUid) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (sketch, BodyUid::new(uid, 0))
}

/// A plate (`Plate:1`, O1, at the origin) and a pin (`Pin:1`, O2, at x =
/// 100), each a block; the root is active.
struct Rig {
    doc: Document<MockKernel>,
    plate: ComponentUid,
    o1: OccurrenceUid,
    pin: ComponentUid,
    o2: OccurrenceUid,
    plate_sketch: FeatureUid,
    plate_body: BodyUid,
    pin_body: BodyUid,
}

fn rig() -> Rig {
    let mut doc = Document::new(MockKernel::default());
    let (plate, o1) = doc
        .create_component(Some("Plate"), Transform::IDENTITY, true)
        .unwrap();
    let (plate_sketch, plate_body) = block_in(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (pin, o2) = doc
        .create_component(Some("Pin"), shift(100.0, 0.0, 0.0), true)
        .unwrap();
    let (_, pin_body) = block_in(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    Rig {
        doc,
        plate,
        o1,
        pin,
        o2,
        plate_sketch,
        plate_body,
        pin_body,
    }
}

impl Rig {
    fn run(&mut self, cmd: Value) -> Value {
        command(&mut self.doc, cmd)
    }

    fn refuse(&mut self, cmd: Value) -> String {
        refused(&mut self.doc, cmd)
    }

    fn face(&self, body: BodyUid, role: &str) -> String {
        self.faces(body, role).remove(0)
    }

    fn faces(&self, body: BodyUid, role: &str) -> Vec<String> {
        self.doc
            .body_shape(body)
            .unwrap()
            .faces
            .iter()
            .filter(|f| f.role == role)
            .map(ToString::to_string)
            .collect()
    }

    /// The plate's top face (b of most joints).
    fn plate_top(&self) -> Value {
        json!({"occurrence": "Plate:1",
               "geometry": {"body": self.plate_body, "face": self.face(self.plate_body, "end")}})
    }

    /// The pin's bottom face (a of most joints).
    fn pin_bottom(&self) -> Value {
        json!({"occurrence": "Pin:1",
               "geometry": {"body": self.pin_body, "face": self.face(self.pin_body, "start")}})
    }

    fn placement(&self, o: OccurrenceUid) -> Transform {
        self.doc.placement(o).unwrap()
    }

    fn ground(&mut self, o: OccurrenceUid, grounded: bool) {
        self.doc.set_occurrence_grounded(o, grounded).unwrap();
    }

    /// Adds a joint of the pin's bottom on the plate's top with `extra`
    /// fields; returns its uid.
    fn joint(&mut self, kind: &str, extra: Value) -> FeatureUid {
        let mut cmd = json!({"cmd": "add_joint", "kind": kind,
                             "a": self.pin_bottom(), "b": self.plate_top()});
        for (key, value) in extra.as_object().unwrap() {
            cmd[key] = value.clone();
        }
        let result = self.run(cmd);
        assert_eq!(result["error"], Value::Null, "{result}");
        result["uid"].as_str().unwrap().parse().unwrap()
    }

    fn joint_json(&self, uid: FeatureUid) -> Value {
        let joints = query(&self.doc, json!({"query": "joints"}));
        joints["joints"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["uid"] == uid.to_string())
            .unwrap()
            .clone()
    }
}

#[test]
fn every_kind_has_its_free_motions() {
    use Motion as M;
    let z = SlideAxis::Z;
    assert_eq!(JointKind::Rigid.motions(z), vec![]);
    assert_eq!(JointKind::Revolute.motions(z), vec![M::Rz]);
    assert_eq!(JointKind::Slider.motions(SlideAxis::X), vec![M::Tx]);
    assert_eq!(JointKind::Slider.motions(SlideAxis::Y), vec![M::Ty]);
    assert_eq!(JointKind::Slider.motions(z), vec![M::Tz]);
    assert_eq!(JointKind::Cylindrical.motions(z), vec![M::Tz, M::Rz]);
    assert_eq!(JointKind::PinSlot.motions(z), vec![M::Tx, M::Rz]);
    assert_eq!(JointKind::Planar.motions(z), vec![M::Tx, M::Ty, M::Rz]);
    assert_eq!(JointKind::Ball.motions(z), vec![M::Rz, M::Ry, M::Rx]);
    let constraints: Vec<u32> = JointKind::ALL.iter().map(|k| k.constraints()).collect();
    assert_eq!(constraints, [6, 5, 5, 4, 4, 3, 3]);

    // Each kind's motions at some values read back as those values, and
    // a motion the kind does not have is no position of it.
    let sample = BTreeMap::from([
        (M::Tx, 3.0),
        (M::Ty, -2.0),
        (M::Tz, 7.5),
        (M::Rx, 0.3),
        (M::Ry, -0.4),
        (M::Rz, 1.1),
    ]);
    for kind in JointKind::ALL {
        for slide in [SlideAxis::X, SlideAxis::Y, SlideAxis::Z] {
            if kind != JointKind::Slider && slide != z {
                continue;
            }
            let motions = kind.motions(slide);
            let values: BTreeMap<Motion, f64> = motions.iter().map(|m| (*m, sample[m])).collect();
            let t = motion_transform(kind, slide, &values);
            let read =
                motion_values(kind, slide, &t).unwrap_or_else(|| panic!("{kind} {slide:?}: {t:?}"));
            for m in &motions {
                assert!((read[m] - values[m]).abs() < 1e-9, "{kind} {m}: {read:?}");
            }
            for other in M::ALL.into_iter().filter(|m| !motions.contains(m)) {
                // The other motion alone, as a kind that has it gives it.
                let has_it = match other {
                    M::Rx | M::Ry | M::Rz => JointKind::Ball,
                    M::Tx | M::Ty => JointKind::Planar,
                    M::Tz => JointKind::Cylindrical,
                };
                let alone = motion_transform(has_it, z, &BTreeMap::from([(other, 0.5)]));
                let moved = alone.after(&t);
                assert!(
                    motion_values(kind, slide, &moved).is_none(),
                    "{kind} {slide:?} allows {other}"
                );
            }
        }
    }
}

#[test]
fn a_joint_to_a_grounded_occurrence_places_the_free_one() {
    let mut rig = rig();
    // Neither side grounded: side a (the pin) moves onto side b.
    let uid = rig.joint("rigid", json!({"flip": true}));
    assert_near(rig.placement(rig.o2).translation, [0.0, 0.0, 5.0]);
    assert_near(rig.placement(rig.o1).translation, [0.0; 3]);
    let joint = rig.joint_json(uid);
    assert_eq!(joint["state"], "placed", "{joint}");
    assert_eq!(joint["solved"], true);
    assert_eq!(joint["status"], "ok");
    assert_eq!(joint["values"], json!({}));
    // The occurrence's own placement stays where it was placed.
    assert_near(
        rig.doc
            .assembly()
            .occurrence(rig.o2)
            .unwrap()
            .transform
            .translation,
        [100.0, 0.0, 0.0],
    );

    // Grounding the plate keeps it there: the pin's bottom on the plate's
    // top, turned over so that the faces meet.
    rig.ground(rig.o1, true);
    let placed = rig.placement(rig.o2);
    assert!(placed.is_rigid());
    assert_near(placed.translation, [0.0, 0.0, 5.0]);
    assert_near(placed.apply_vector([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
    let joint = rig.joint_json(uid);
    assert_eq!(joint["state"], "placed", "{joint}");
    assert_eq!(joint["moved"], json!(["O2"]));
    assert_eq!(joint["values"], json!({}));
    assert_eq!(joint["a"]["occurrence"], "Pin:1");
    assert_eq!(joint["b"]["path"], json!(["O1"]));
    // Both frames at the plate's top, in the root's coordinates.
    let frames = &joint["frames"];
    assert_near(json_vec(&frames["a"]["origin"]), [5.0, 5.0, 5.0]);
    assert_near(json_vec(&frames["b"]["origin"]), [5.0, 5.0, 5.0]);
    assert_near(json_vec(&frames["a"]["z_axis"]), [0.0, 0.0, -1.0]);
    assert_near(json_vec(&frames["b"]["z_axis"]), [0.0, 0.0, 1.0]);
    // The instances show the pin there.
    let instances = query(&rig.doc, json!({"query": "instances"}));
    let pin = instances
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["occurrence"] == "Pin:1")
        .unwrap();
    assert!((pin["transform"][2][3].as_f64().unwrap() - 5.0).abs() < 1e-9);

    // A revolute joint at rest a quarter turn: the pin turns about the
    // plate's normal through the faces' middles.
    let mut rig2 = self::rig();
    rig2.ground(rig2.o1, true);
    let uid = rig2.joint(
        "revolute",
        json!({"flip": true, "limits": {"rz": {"rest": "90 deg"}}}),
    );
    let placed = rig2.placement(rig2.o2);
    assert_near(placed.apply_point([5.0, 5.0, 0.0]), [5.0, 5.0, 5.0]);
    assert_near(placed.apply_vector([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);
    let joint = rig2.joint_json(uid);
    assert!((joint["values"]["rz"].as_f64().unwrap() - FRAC_PI_2).abs() < 1e-9);
    assert_eq!(joint["motions"], json!(["rz"]));
    assert_eq!(joint["limits"]["rz"]["rest"], json!(FRAC_PI_2));
}

#[test]
fn either_side_can_be_the_free_one() {
    let mut rig = rig();
    rig.ground(rig.o2, true);
    let uid = rig.joint("rigid", json!({"flip": true}));
    // The plate moves under the pin: its top's middle at the pin's bottom.
    assert_near(rig.placement(rig.o1).translation, [100.0, 0.0, -5.0]);
    assert_near(rig.placement(rig.o2).translation, [100.0, 0.0, 0.0]);
    assert_eq!(rig.joint_json(uid)["moved"], json!(["O1"]));
    // Both grounded: the joint holds or fails, nothing moves.
    rig.ground(rig.o1, true);
    let joint = rig.joint_json(uid);
    assert_eq!(joint["state"], "failed", "{joint}");
    assert_eq!(
        joint["error"],
        "both sides are fixed (Pin:1 and Plate:1) and the joint does not hold"
    );
    assert_near(rig.placement(rig.o1).translation, [0.0, 0.0, 0.0]);
    // An offset and an angle along and about the joint's z axis.
    rig.ground(rig.o2, false);
    let edited = rig.joint_json(uid);
    let mut def = query(&rig.doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    def["offset"] = json!(2);
    def["angle"] = json!("180 deg");
    rig.run(json!({"cmd": "edit_feature", "uid": uid, "def": def}));
    assert_eq!(edited["state"], "placed");
    let placed = rig.placement(rig.o2);
    assert_near(placed.apply_point([5.0, 5.0, 0.0]), [5.0, 5.0, 7.0]);
    assert_near(placed.apply_vector([1.0, 0.0, 0.0]), [-1.0, 0.0, 0.0]);
}

#[test]
fn origins_resolve_on_each_kind_of_geometry() {
    let mut rig = rig();
    let body = rig.plate_body;
    let sides = rig.faces(body, "side");
    let kernel = rig.doc.kernel();
    kernel.surfaces.borrow_mut().insert(
        sides[0].clone(),
        SurfaceGeometry::Cylinder {
            origin: [1.0, 2.0, 3.0],
            axis: [0.0, 1.0, 0.0],
            radius: 2.0,
        },
    );
    kernel.surfaces.borrow_mut().insert(
        sides[1].clone(),
        SurfaceGeometry::Sphere {
            center: [4.0, 4.0, 4.0],
            radius: 1.0,
        },
    );
    kernel.surfaces.borrow_mut().insert(
        sides[2].clone(),
        SurfaceGeometry::Cone {
            origin: [0.0, 0.0, 1.0],
            axis: [1.0, 0.0, 0.0],
            radius: 1.0,
            half_angle: 0.1,
        },
    );
    let shape = rig.doc.body_shape(body).unwrap().clone();
    let edge = |i: usize| -> String {
        let (a, b) = &shape.edges[i];
        format!("E{{{a}|{b}}}")
            .parse::<EdgeName>()
            .unwrap()
            .to_string()
    };
    let (circle, line) = (edge(0), edge(1));
    kernel.edge_curves.borrow_mut().insert(
        circle.clone(),
        CurveGeometry::Circle {
            center: [5.0, 5.0, 5.0],
            normal: [0.0, 0.0, -1.0],
            radius: 3.0,
            start: [8.0, 5.0, 5.0],
            end: [8.0, 5.0, 5.0],
        },
    );
    let top = rig.face(body, "end");
    let vertex = format!("V{{{}|{}|{}}}", sides[0], sides[1], top)
        .parse::<VertexName>()
        .unwrap()
        .to_string();
    kernel
        .vertex_points
        .borrow_mut()
        .insert(vertex.clone(), [10.0, 10.0, 5.0]);
    let sketch = rig.plate_sketch;
    let output = rig.doc.sketch_output(sketch).unwrap();
    let (point, at) = output
        .solved
        .points
        .iter()
        .next()
        .map(|(p, at)| (*p, *at))
        .unwrap();
    rig.doc.activate_component(rig.plate).unwrap();
    let circles = rig.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let circle_curve = rig
        .doc
        .add_circle(circles, [3.0, 4.0], &num(2.0))
        .unwrap()
        .curves[0];

    let x = [1.0, 0.0, 0.0];
    let y = [0.0, 1.0, 0.0];
    let z = [0.0, 0.0, 1.0];
    let cases: Vec<(Value, Vec3, Vec3, Vec3)> = vec![
        // A planar face: its middle, its outward normal.
        (json!({"body": body, "face": top}), [5.0, 5.0, 5.0], x, z),
        // A cylinder: its axis; a sphere: its centre and the axes.
        (
            json!({"body": body, "face": sides[0]}),
            [1.0, 2.0, 3.0],
            x,
            y,
        ),
        (
            json!({"body": body, "face": sides[1]}),
            [4.0, 4.0, 4.0],
            x,
            z,
        ),
        // A cone along X: x is then the model Y.
        (
            json!({"body": body, "face": sides[2]}),
            [0.0, 0.0, 1.0],
            y,
            x,
        ),
        // A circular edge: its centre, the axis along its largest part.
        (json!({"body": body, "edge": circle}), [5.0, 5.0, 5.0], x, z),
        // A straight edge (the mock's run along X): its middle.
        (json!({"body": body, "edge": line}), [0.5, 0.0, 0.0], y, x),
        (
            json!({"body": body, "vertex": vertex}),
            [10.0, 10.0, 5.0],
            x,
            z,
        ),
        (
            json!({"sketch": sketch, "point": format!("p{}", point.0)}),
            [at[0], at[1], 0.0],
            x,
            z,
        ),
        (
            json!({"sketch": circles, "curve": format!("c{}", circle_curve.0)}),
            [3.0, 4.0, 0.0],
            x,
            z,
        ),
        (json!("xz"), [0.0; 3], x, y),
        (json!("z"), [0.0; 3], x, z),
        (json!({"point": [1, 2, 3]}), [1.0, 2.0, 3.0], x, z),
    ];
    for (i, (geometry, origin, x_axis, normal)) in cases.into_iter().enumerate() {
        let added = rig.run(
            json!({"cmd": "add_feature", "def": {"type": "joint_origin", "geometry": geometry}}),
        );
        assert_eq!(added["error"], Value::Null, "case {i}: {added}");
        assert!(added["name"].as_str().unwrap().starts_with("JointOrigin"));
        let uid: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
        let Some(Datum::Plane(plane)) = rig.doc.datum(uid) else {
            panic!("case {i}: {geometry} gave no plane");
        };
        assert!(near(plane.origin, origin), "case {i}: {plane:?}");
        assert!(near(plane.x_axis, x_axis), "case {i}: {plane:?}");
        assert!(near(plane.normal(), normal), "case {i}: {plane:?}");
    }

    // The frame's parts given directly, then moved by offset, angle and
    // flip.
    let added = rig.run(json!({"cmd": "add_feature", "def": {"type": "joint_origin",
               "geometry": {"body": body, "face": top},
               "frame_override": {"origin": [0, 0, 0], "z_axis": [1, 0, 0]}}}));
    let uid: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
    let Some(Datum::Plane(plane)) = rig.doc.datum(uid) else {
        panic!("no plane");
    };
    assert_near(plane.origin, [0.0; 3]);
    assert_near(plane.normal(), x);
    assert_near(plane.x_axis, y);
    let added = rig.run(json!({"cmd": "add_feature", "def": {"type": "joint_origin",
               "geometry": {"body": body, "face": top},
               "offset": 2, "angle": "90 deg", "flip": true}}));
    assert_eq!(added["parameters"].as_array().unwrap().len(), 2);
    let uid: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
    let Some(Datum::Plane(plane)) = rig.doc.datum(uid) else {
        panic!("no plane");
    };
    assert_near(plane.origin, [5.0, 5.0, 7.0]);
    assert_near(plane.x_axis, y);
    assert_near(plane.normal(), [0.0, 0.0, -1.0]);
    // A joint origin's datum is listed with the others and can carry a
    // sketch.
    let datums = query(&rig.doc, json!({"query": "datums"}));
    assert!(datums.to_string().contains(&uid.to_string()));
    assert!(rig.doc.add_sketch(SketchPlane::Construction(uid)).is_ok());

    // A body gives no frame.
    let error = rig.refuse(
        json!({"cmd": "add_feature", "def": {"type": "joint_origin", "geometry": {"body": body}}}),
    );
    assert!(error.contains("gives no frame"), "{error}");
}

#[test]
fn origins_are_geometry_of_their_occurrence_s_component() {
    let mut rig = rig();
    rig.ground(rig.o1, true);
    // A joint origin in the pin: its bottom, turned over.
    rig.doc.activate_component(rig.pin).unwrap();
    let bottom = rig.face(rig.pin_body, "start");
    let added = rig.run(json!({"cmd": "add_feature", "def": {"type": "joint_origin",
               "geometry": {"body": rig.pin_body, "face": bottom}, "flip": true}}));
    let pin_origin = added["uid"].as_str().unwrap().to_owned();
    rig.doc.activate_component(ComponentUid::ROOT).unwrap();
    // The root's joint uses it through the pin's occurrence.
    let result = rig.run(json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": "O2", "geometry": pin_origin},
               "b": rig.plate_top()}));
    assert_eq!(result["state"], "placed", "{result}");
    assert_near(rig.placement(rig.o2).translation, [0.0, 0.0, 5.0]);
    let uid: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    // Deleting the joint origin asks about the joint.
    let error = rig.refuse(json!({"cmd": "delete_feature", "uid": pin_origin}));
    assert!(
        error.contains(&rig.doc.feature(uid).unwrap().name),
        "{error}"
    );

    // The plate's sketch is not geometry of the pin.
    let error = rig.refuse(json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": "Pin:1",
                     "geometry": {"sketch": rig.plate_sketch, "point": "p5"}},
               "b": rig.plate_top()}));
    assert!(
        error.contains("a: Sketch1 is in Plate, not in Pin"),
        "{error}"
    );
    // Nor is the plate's body: recompute says so.
    let result = rig.run(json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": "Pin:1",
                     "geometry": {"body": rig.plate_body, "face": rig.face(rig.plate_body, "start")}},
               "b": rig.plate_top()}),
    );
    let error = result["error"].as_str().unwrap();
    assert!(error.contains("is not in Pin"), "{error}");
    let joint = rig.joint_json(result["uid"].as_str().unwrap().parse().unwrap());
    assert_eq!(joint["state"], "failed");
    // Unknown occurrences, the same occurrence twice, both sides the root.
    let error = rig.refuse(json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": "Pin:7", "geometry": "xy"}, "b": rig.plate_top()}));
    assert!(error.contains("no occurrence Pin:7 in Root"), "{error}");
    let error = rig.refuse(
        json!({"cmd": "add_joint", "kind": "rigid", "a": rig.plate_top(), "b": rig.plate_top()}),
    );
    assert!(error.contains("same occurrence"), "{error}");
    let error = rig.refuse(
        json!({"cmd": "add_joint", "kind": "rigid", "a": rig.pin_bottom(),
                                  "b": rig.plate_top(), "flipped": true}),
    );
    assert!(error.contains("unknown field `flipped`"), "{error}");
    let error = rig.refuse(json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"geometry": "xy"}, "b": {"geometry": "origin"}}));
    assert!(
        error.contains("both the component's own geometry"),
        "{error}"
    );
    // A joint to the root's own geometry: the root is fixed.
    let mut rig = self::rig();
    let result = rig.run(
        json!({"cmd": "add_joint", "kind": "rigid", "a": rig.pin_bottom(),
               "b": {"geometry": {"point": [0, 0, 50]}}}),
    );
    assert_eq!(result["state"], "placed", "{result}");
    // The pin's bottom (z down) at the point: turned over about x.
    let placed = rig.placement(rig.o2);
    assert_near(placed.translation, [-5.0, 5.0, 50.0]);
    assert_near(placed.apply_point([5.0, 5.0, 0.0]), [0.0, 0.0, 50.0]);
}

#[test]
fn limits_are_values_and_expressions() {
    let mut rig = rig();
    rig.ground(rig.o1, true);
    rig.run(json!({"cmd": "add_parameter", "name": "travel", "value": 20}));
    let result = rig.run(
        json!({"cmd": "add_joint", "kind": "slider", "a": rig.pin_bottom(), "b": rig.plate_top(),
               "flip": true,
               "limits": {"tz": {"min": 0, "max": "travel * 2", "rest": "travel / 2"}}}),
    );
    assert_eq!(
        result["parameters"].as_array().unwrap().len(),
        3,
        "{result}"
    );
    let uid: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    // At rest 10 mm above the plate's top, along its normal.
    assert_near(rig.placement(rig.o2).translation, [0.0, 0.0, 15.0]);
    let joint = rig.joint_json(uid);
    assert_eq!(
        joint["limits"]["tz"],
        json!({"min": 0.0, "max": 40.0, "rest": 10.0})
    );
    assert!((joint["values"]["tz"].as_f64().unwrap() - 10.0).abs() < 1e-9);
    assert_eq!(joint["within_limits"], true);
    // The limits follow their parameters.
    rig.run(json!({"cmd": "set_parameter", "name": "travel", "value": 30}));
    assert_near(rig.placement(rig.o2).translation, [0.0, 0.0, 20.0]);
    // The parameters are the joint's, named by slot.
    let parameters = query(&rig.doc, json!({"query": "parameters"}));
    let comments: Vec<&str> = parameters
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["owner"] == uid.to_string())
        .map(|p| p["comment"].as_str().unwrap())
        .collect();
    assert_eq!(comments, ["Joint1 min", "Joint1 max", "Joint1 rest"]);

    // A rest outside the limits is refused, or fails when a parameter
    // takes it there.
    let mut def = query(&rig.doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    def["limits"]["tz"]["rest"] = json!("travel * 3");
    let error = rig.refuse(json!({"cmd": "edit_feature", "uid": uid, "def": def}));
    assert!(
        error.contains("rest value is outside the limits"),
        "{error}"
    );
    def["limits"]["tz"]["min"] = json!(12);
    def["limits"]["tz"]["rest"] = json!("travel / 2");
    rig.run(json!({"cmd": "edit_feature", "uid": uid, "def": def}));
    rig.run(json!({"cmd": "set_parameter", "name": "travel", "value": 20}));
    assert!(
        matches!(rig.doc.status(uid), Some(FeatureStatus::Failed(m)) if m.contains("outside the limits")),
        "{:?}",
        rig.doc.status(uid)
    );
    // A failed joint moves nothing.
    assert_near(rig.placement(rig.o2).translation, [100.0, 0.0, 0.0]);

    // Angles take angle parameters; motions the kind lacks are refused.
    let error = rig.refuse(
        json!({"cmd": "add_joint", "kind": "revolute", "a": rig.pin_bottom(),
               "b": rig.plate_top(), "limits": {"rz": {"max": "travel"}}}),
    );
    assert!(
        error.contains("travel' is a length, not an angle"),
        "{error}"
    );
    let error = rig.refuse(
        json!({"cmd": "add_joint", "kind": "revolute", "a": rig.pin_bottom(),
               "b": rig.plate_top(), "limits": {"tz": {"max": 1}}}),
    );
    assert!(
        error.contains("a revolute joint has no free motion tz"),
        "{error}"
    );
    let error = rig.refuse(
        json!({"cmd": "add_joint", "kind": "revolute", "a": rig.pin_bottom(),
               "b": rig.plate_top(), "slide_axis": "x"}),
    );
    assert!(error.contains("slide_axis is for slider joints"), "{error}");

    // Values beyond the limits where both sides are fixed.
    let mut rig = self::rig();
    rig.doc
        .set_occurrence_transform(rig.o2, shift(0.0, 0.0, 25.0), false)
        .unwrap();
    rig.ground(rig.o1, true);
    rig.ground(rig.o2, true);
    let uid = rig.joint(
        "slider",
        json!({"flip": true, "limits": {"tz": {"max": 15}}}),
    );
    let joint = rig.joint_json(uid);
    assert_eq!(joint["state"], "satisfied", "{joint}");
    assert!((joint["values"]["tz"].as_f64().unwrap() - 20.0).abs() < 1e-9);
    assert_eq!(joint["within_limits"], false);
    assert_eq!(joint["beyond_limits"], json!(["tz"]));
}

#[test]
fn as_built_joints_keep_the_recorded_placement() {
    let mut rig = rig();
    let turned = shift(100.0, 0.0, 0.0)
        .after(&Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], PI / 6.0).unwrap());
    rig.doc
        .set_occurrence_transform(rig.o2, turned, false)
        .unwrap();
    let top = rig.face(rig.pin_body, "end");
    let result = rig.run(
        json!({"cmd": "add_as_built_joint", "kind": "revolute", "a": "Pin:1", "b": "Plate:1",
               "origin": {"occurrence": "Pin:1", "geometry": {"body": rig.pin_body, "face": top}},
               "limits": {"rz": {"min": "-45 deg", "max": "45 deg"}}}),
    );
    assert_eq!(result["error"], Value::Null, "{result}");
    assert!(result["name"].as_str().unwrap().starts_with("AsBuiltJoint"));
    let uid: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    // The relation is recorded in the definition.
    let def = query(&rig.doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    assert_eq!(def["a"], "O2");
    assert_eq!(def["relative"][0][3], 100.0);
    // It holds where it was made: nothing moves.
    let joint = rig.joint_json(uid);
    assert_eq!(joint["state"], "satisfied");
    assert_eq!(joint["values"], json!({"rz": 0.0}));
    // Where the plate goes, the pin follows.
    rig.doc
        .set_occurrence_transform(rig.o1, shift(0.0, 50.0, 0.0), false)
        .unwrap();
    rig.ground(rig.o1, true);
    let placed = rig.placement(rig.o2);
    assert_near(placed.translation, [100.0, 50.0, 0.0]);
    assert_near(
        placed.apply_vector([1.0, 0.0, 0.0]),
        turned.apply_vector([1.0, 0.0, 0.0]),
    );
    let joint = rig.joint_json(uid);
    assert_eq!(joint["state"], "placed");
    assert_eq!(joint["within_limits"], true);
    // The motion frame is the pin's top, in the root's coordinates.
    let expected = shift(0.0, 50.0, 0.0)
        .after(&turned)
        .apply_point([5.0, 5.0, 5.0]);
    assert_near(json_vec(&joint["frames"]["a"]["origin"]), expected);
    // Same occurrence twice is refused.
    let error =
        rig.refuse(json!({"cmd": "add_as_built_joint", "kind": "rigid", "a": "O2", "b": "Pin:1"}));
    assert!(error.contains("same occurrence"), "{error}");
}

#[test]
fn rigid_groups_move_as_one_and_count_as_one() {
    let mut rig = rig();
    let o3 = rig
        .doc
        .copy_occurrence(rig.o2, Some(shift(200.0, 0.0, 0.0)))
        .unwrap();
    let group = rig.run(json!({"cmd": "add_rigid_group", "occurrences": ["Pin:1", "Pin:2"]}));
    assert_eq!(group["error"], Value::Null, "{group}");
    assert_eq!(group["name"], "RigidGroup1");
    rig.ground(rig.o1, true);
    let uid = rig.joint("rigid", json!({"flip": true}));
    // The pin moved onto the plate and its group mate by the same motion.
    assert_near(rig.placement(rig.o2).translation, [0.0, 0.0, 5.0]);
    assert_near(rig.placement(o3).translation, [100.0, 0.0, 5.0]);
    let joint = rig.joint_json(uid);
    assert_eq!(joint["moved"], json!(["O2", "O3"]));
    let joints = query(&rig.doc, json!({"query": "joints"}));
    assert_eq!(
        joints["rigid_groups"][0]["names"],
        json!(["Pin:1", "Pin:2"])
    );
    assert_eq!(joints["rigid_groups"][0]["active"], true);
    let dof = &joints["dof"][0];
    assert_eq!(dof["total"], 0, "{dof}");
    assert_eq!(dof["units"][1]["occurrences"], json!(["O2", "O3"]));
    assert_eq!(dof["units"][1]["dof"], 0);
    // Joined to a grounded occurrence, the group is fixed.
    let error = rig.refuse(json!({"cmd": "add_rigid_group", "occurrences": ["Pin:1"]}));
    assert!(error.contains("at least two occurrences"), "{error}");
    let error =
        rig.refuse(json!({"cmd": "add_rigid_group", "occurrences": ["Pin:1", "Plate:1/X"]}));
    assert!(error.contains("no occurrence Plate:1/X"), "{error}");
}

#[test]
fn degrees_of_freedom_follow_kinds_grounding_and_groups() {
    let mut rig = rig();
    let o3 = rig
        .doc
        .copy_occurrence(rig.o2, Some(shift(200.0, 0.0, 0.0)))
        .unwrap();
    rig.ground(rig.o1, true);
    rig.joint("revolute", json!({"flip": true}));
    let joints = query(&rig.doc, json!({"query": "joints"}));
    let dof = &joints["dof"][0];
    assert_eq!(dof["component"], "C0");
    // Pin:1 turns (1), Pin:2 is free (6), the plate is grounded.
    assert_eq!(dof["total"], 7, "{dof}");
    assert_eq!(dof["overconstrained"], false);
    let units = dof["units"].as_array().unwrap();
    assert_eq!(units[0]["grounded"], true);
    assert_eq!(units[0]["dof"], 0);
    assert_eq!(units[1]["dof"], 1);
    assert_eq!(units[2]["dof"], 6);

    let pin = query(
        &rig.doc,
        json!({"query": "joint_dof", "occurrence": "Pin:1"}),
    );
    assert_eq!(pin["motions"], json!(["rz"]), "{pin}");
    assert_eq!(pin["dof"], 1);
    assert_eq!(pin["joints"][0]["other"], "Plate:1");
    assert_eq!(pin["joints"][0]["kind"], "revolute");
    let free = query(
        &rig.doc,
        json!({"query": "joint_dof", "occurrence": o3.to_string()}),
    );
    assert_eq!(free["motions"], json!(["tx", "ty", "tz", "rx", "ry", "rz"]));
    assert_eq!(free["dof"], 6);
    let plate = query(&rig.doc, json!({"query": "joint_dof", "occurrence": "O1"}));
    assert_eq!(plate["motions"], json!([]));
    assert_eq!(plate["grounded"], true);

    // A ball joint of the pin's origin to the plate's top: off the turning
    // axis, it contradicts the revolute joint and fails, naming both.
    let mut cmd = json!({"cmd": "add_joint", "kind": "ball",
                         "a": rig.pin_bottom(), "b": rig.plate_top()});
    cmd["a"]["geometry"] = json!("origin");
    let result = rig.run(cmd);
    assert_eq!(
        result["error"], "Joint2: over-constrained: Joint1 and Joint2 cannot all hold",
        "{result}"
    );
    let joints = query(&rig.doc, json!({"query": "joints"}));
    assert_eq!(joints["joints"][1]["state"], "failed");
    let dof = &joints["dof"][0];
    assert_eq!(dof["units"][1]["dof"], 1);
    assert_eq!(dof["total"], 7);
    assert_eq!(dof["overconstrained"], false);

    // A second revolute joint on the same axis repeats the first: it
    // holds, takes nothing more away and is reported redundant.
    let ball: FeatureUid = result["uid"].as_str().unwrap().parse().unwrap();
    rig.run(json!({"cmd": "delete_feature", "uid": ball}));
    let mut cmd = json!({"cmd": "add_joint", "kind": "revolute", "flip": true,
                         "a": rig.pin_bottom(), "b": rig.plate_top()});
    cmd["a"]["frame_override"] = json!({"origin": [5, 5, -20]});
    cmd["b"]["frame_override"] = json!({"origin": [5, 5, -15]});
    let result = rig.run(cmd);
    assert_eq!(result["error"], Value::Null, "{result}");
    let second = result["uid"].clone();
    let joints = query(&rig.doc, json!({"query": "joints"}));
    let dof = &joints["dof"][0];
    assert_eq!(dof["total"], 7, "{dof}");
    assert_eq!(dof["units"][1]["dof"], 1);
    assert_eq!(dof["overconstrained"], true);
    assert_eq!(dof["redundant"], json!([second]));
    let pin = query(
        &rig.doc,
        json!({"query": "joint_dof", "occurrence": "Pin:1"}),
    );
    assert_eq!(pin["motions"], Value::Null);
    assert_eq!(pin["joints"].as_array().unwrap().len(), 2);
}

#[test]
fn suppress_edit_delete_and_undo() {
    let mut rig = rig();
    rig.ground(rig.o1, true);
    let uid = rig.joint("rigid", json!({"flip": true}));
    let placed = [0.0, 0.0, 5.0];
    assert_near(rig.placement(rig.o2).translation, placed);

    // Suppressed: the pin is back where it was placed.
    rig.run(json!({"cmd": "suppress_feature", "uid": uid}));
    assert_near(rig.placement(rig.o2).translation, [100.0, 0.0, 0.0]);
    let joint = rig.joint_json(uid);
    assert_eq!(joint["state"], "suppressed");
    assert_eq!(joint["solved"], false);
    rig.run(json!({"cmd": "suppress_feature", "uid": uid, "suppressed": false}));
    assert_near(rig.placement(rig.o2).translation, placed);

    // Edited into a slider: the same type, another kind.
    let mut def = query(&rig.doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    assert_eq!(def["kind"], "rigid");
    def["kind"] = json!("slider");
    def["slide_axis"] = json!("x");
    def["limits"] = json!({"tx": {"rest": 3}});
    rig.run(json!({"cmd": "edit_feature", "uid": uid, "def": def}));
    assert_near(rig.placement(rig.o2).translation, [3.0, 0.0, 5.0]);
    let joint = rig.joint_json(uid);
    assert_eq!(joint["kind"], "slider");
    assert_eq!(joint["slide_axis"], "x");
    assert_eq!(joint["motions"], json!(["tx"]));

    // Rolled back: not in effect.
    rig.run(json!({"cmd": "set_marker", "position": 4}));
    assert_eq!(rig.joint_json(uid)["state"], "rolled_back");
    assert_near(rig.placement(rig.o2).translation, [100.0, 0.0, 0.0]);
    rig.run(json!({"cmd": "set_marker", "position": 5}));

    // Deleted, and back with undo.
    let deleted = rig.run(json!({"cmd": "delete_feature", "uid": uid}));
    assert_eq!(deleted["deleted"], json!([uid.to_string()]));
    assert_near(rig.placement(rig.o2).translation, [100.0, 0.0, 0.0]);
    assert_eq!(
        query(&rig.doc, json!({"query": "joints"}))["joints"],
        json!([])
    );
    let undone = rig.run(json!({"cmd": "undo"}));
    assert_eq!(undone["label"], "Delete Joint1");
    assert_near(rig.placement(rig.o2).translation, [3.0, 0.0, 5.0]);
    // Back past the marker moves and the edit.
    for label in [
        "Move Timeline Marker",
        "Move Timeline Marker",
        "Edit Joint1",
    ] {
        assert_eq!(rig.run(json!({"cmd": "undo"}))["label"], label);
    }
    assert_eq!(rig.joint_json(uid)["kind"], "rigid");
    assert_near(rig.placement(rig.o2).translation, placed);
    for _ in 0..4 {
        rig.run(json!({"cmd": "redo"}));
    }
    assert_eq!(
        query(&rig.doc, json!({"query": "joints"}))["joints"],
        json!([])
    );
}

#[test]
fn deleting_an_occurrence_deletes_its_joints() {
    let mut rig = rig();
    rig.ground(rig.o1, true);
    let uid = rig.joint("rigid", json!({"flip": true}));
    let o3 = rig
        .doc
        .copy_occurrence(rig.o2, Some(shift(0.0, 80.0, 0.0)))
        .unwrap();
    let group = rig.run(json!({"cmd": "add_rigid_group", "occurrences": ["O1", o3.to_string()]}));
    let deleted = rig.run(json!({"cmd": "delete_occurrence", "occurrence": "Pin:1"}));
    assert_eq!(deleted["deleted"], json!([uid.to_string()]), "{deleted}");
    assert!(
        rig.doc
            .feature(group["uid"].as_str().unwrap().parse().unwrap())
            .is_some()
    );
    rig.run(json!({"cmd": "undo"}));
    assert_eq!(rig.joint_json(uid)["state"], "placed");
    // The last occurrence of the pin takes the component, its features
    // and the joints and groups of its occurrences.
    rig.run(json!({"cmd": "delete_occurrence", "occurrence": "Pin:1"}));
    let deleted = rig.run(json!({"cmd": "delete_occurrence", "occurrence": o3.to_string()}));
    let deleted = deleted["deleted"].as_array().unwrap();
    assert!(deleted.contains(&group["uid"]), "{deleted:?}");
    assert_eq!(rig.doc.assembly().occurrences_of(rig.pin).count(), 0);
}

#[test]
fn joints_survive_the_project_file() {
    let mut rig = rig();
    rig.ground(rig.o1, true);
    rig.run(json!({"cmd": "add_parameter", "name": "lift", "value": 4}));
    rig.joint(
        "cylindrical",
        json!({"flip": true, "offset": "lift", "angle": "30 deg",
               "limits": {"tz": {"min": -5, "max": 5, "rest": "lift / 2"},
                          "rz": {"rest": "45 deg"}}}),
    );
    rig.doc.activate_component(rig.pin).unwrap();
    rig.run(
        json!({"cmd": "add_feature", "def": {"type": "joint_origin", "geometry": "xy",
               "frame_override": {"origin": [1, 2, 3]}, "offset": 1}}),
    );
    rig.doc.activate_component(ComponentUid::ROOT).unwrap();
    let o3 = rig
        .doc
        .copy_occurrence(rig.o2, Some(shift(0.0, 80.0, 0.0)))
        .unwrap();
    rig.run(json!({"cmd": "add_rigid_group", "occurrences": ["Pin:1", o3.to_string()]}));
    rig.run(json!({"cmd": "add_as_built_joint", "kind": "planar", "a": "Pin:2", "b": "Plate:1"}));
    let text = rig.doc.to_json();
    for t in [
        "\"joint\"",
        "\"as_built_joint\"",
        "\"joint_origin\"",
        "\"rigid_group\"",
    ] {
        assert!(text.contains(t), "{t} in {text}");
    }
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    let before = query(&rig.doc, json!({"query": "joints"}));
    let after = query(&copy, json!({"query": "joints"}));
    assert_eq!(before, after);
    assert_eq!(copy.to_json(), text);
    for o in [rig.o2, o3] {
        assert_eq!(copy.placement(o), rig.doc.placement(o));
    }
    assert_eq!(after["joints"][0]["state"], "placed");
    let features = query(&rig.doc, json!({"query": "timeline"}));
    let copied = query(&copy, json!({"query": "timeline"}));
    assert_eq!(features, copied);
}

#[test]
fn copies_of_components_take_their_joints_along() {
    let mut doc = Document::new(MockKernel::default());
    let (sub, sub1) = doc
        .create_component(Some("Sub"), Transform::IDENTITY, true)
        .unwrap();
    let (_, base1) = doc
        .create_component(Some("Base"), Transform::IDENTITY, true)
        .unwrap();
    let (_, base_body) = block_in(&mut doc);
    doc.activate_component(sub).unwrap();
    doc.create_component(Some("Top"), shift(50.0, 0.0, 0.0), true)
        .unwrap();
    let (_, top_body) = block_in(&mut doc);
    doc.activate_component(sub).unwrap();
    doc.set_occurrence_grounded(base1, true).unwrap();
    let faces = |body: BodyUid, role: &str| {
        doc.body_shape(body)
            .unwrap()
            .faces
            .iter()
            .find(|f| f.role == role)
            .unwrap()
            .to_string()
    };
    let (start, end) = (faces(top_body, "start"), faces(base_body, "end"));
    let added = command(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid", "flip": true,
               "a": {"occurrence": "Top:1", "geometry": {"body": top_body, "face": start}},
               "b": {"occurrence": "Base:1", "geometry": {"body": base_body, "face": end}}}),
    );
    assert_eq!(added["state"], "placed", "{added}");
    let original: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (copy, _) = doc.paste_new(sub1, Some(shift(0.0, 100.0, 0.0))).unwrap();
    let joints = query(&doc, json!({"query": "joints"}));
    let joints = joints["joints"].as_array().unwrap();
    assert_eq!(joints.len(), 2, "{joints:?}");
    let copied = joints
        .iter()
        .find(|j| j["uid"] != original.to_string())
        .unwrap();
    assert_eq!(copied["component"], copy.to_string());
    assert_eq!(copied["state"], "placed", "{copied}");
    // The copy's joint names the copy's occurrences.
    let children: Vec<String> = doc
        .assembly()
        .children(copy)
        .map(|o| o.uid.to_string())
        .collect();
    assert!(children.contains(&copied["a"]["path"][0].as_str().unwrap().to_owned()));
    assert!(children.contains(&copied["b"]["path"][0].as_str().unwrap().to_owned()));
}

fn preview(doc: &mut Document<MockKernel>, def: Value) -> Value {
    let text = json!({"cmd": "add_feature", "def": def}).to_string();
    serde_json::from_str(&doc.preview(&text).unwrap()).unwrap()
}

#[test]
fn a_preview_reports_the_occurrences_it_places() {
    let mut rig = rig();
    rig.ground(rig.o1, true);
    // A joint previewed: the pin's new placement in the design, nothing
    // committed.
    let def = json!({"type": "joint", "kind": "rigid", "flip": true,
                     "a": {"occurrence": rig.o2.to_string(), "geometry": rig.pin_bottom()["geometry"]},
                     "b": {"occurrence": rig.o1.to_string(), "geometry": rig.plate_top()["geometry"]}});
    let report = preview(&mut rig.doc, def);
    assert_eq!(report["status"], "ok", "{report}");
    let placements = report["placements"].as_array().unwrap();
    assert_eq!(placements.len(), 1, "{report}");
    assert_eq!(placements[0]["path"], rig.o2.to_string());
    // 4 x 4, in the design: the pin's bottom on the plate's top.
    assert!((placements[0]["transform"][2][3].as_f64().unwrap() - 5.0).abs() < 1e-9);
    assert!(placements[0]["transform"][0][3].as_f64().unwrap().abs() < 1e-9);
    assert_near(rig.placement(rig.o2).translation, [100.0, 0.0, 0.0]);

    // A feature that places nothing reports no placements.
    let plane = json!({"type": "construction_plane",
                       "definition": {"type": "offset", "plane": "xy", "distance": 5}});
    let report = preview(&mut rig.doc, plane);
    assert_eq!(report["status"], "ok", "{report}");
    assert!(report.get("placements").is_none(), "{report}");

    // An occurrence inside a moved one is listed with its own path.
    let mut doc = Document::new(MockKernel::default());
    let (_, outer) = doc
        .create_component(Some("Outer"), Transform::IDENTITY, true)
        .unwrap();
    let (_, inner) = doc
        .create_component(Some("Inner"), shift(1.0, 0.0, 0.0), true)
        .unwrap();
    block_in(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let def = json!({"type": "move_occurrence", "occurrences": [outer.to_string()],
                     "transform": {"type": "translate_xyz", "x": 0, "y": 20, "z": 0}});
    let report = preview(&mut doc, def);
    assert_eq!(report["status"], "ok", "{report}");
    let placements = report["placements"].as_array().unwrap();
    let paths: Vec<&str> = placements
        .iter()
        .map(|p| p["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, [outer.to_string(), format!("{outer}/{inner}")]);
    // The inner one where both put it.
    assert!((placements[1]["transform"][0][3].as_f64().unwrap() - 1.0).abs() < 1e-9);
    assert!((placements[1]["transform"][1][3].as_f64().unwrap() - 20.0).abs() < 1e-9);
}

#[test]
fn joint_frame_gives_an_origin_s_frame_in_the_design() {
    let mut rig = rig();
    // The pin's bottom: its middle, the outward normal, placed by Pin:1.
    let frame = query(
        &rig.doc,
        json!({"query": "joint_frame", "occurrence": "Pin:1",
               "geometry": rig.pin_bottom()["geometry"]}),
    );
    assert_near(json_vec(&frame["origin"]), [105.0, 5.0, 0.0]);
    assert_near(json_vec(&frame["z_axis"]), [0.0, 0.0, -1.0]);
    assert_near(json_vec(&frame["local"]["origin"]), [5.0, 5.0, 0.0]);
    assert_eq!(frame["component"], rig.pin.to_string());
    // By uid, with an override.
    let frame = query(
        &rig.doc,
        json!({"query": "joint_frame", "occurrence": rig.o1.to_string(),
               "geometry": rig.plate_top()["geometry"],
               "frame_override": {"origin": [1, 2, 5]}}),
    );
    assert_near(json_vec(&frame["origin"]), [1.0, 2.0, 5.0]);
    assert_near(json_vec(&frame["z_axis"]), [0.0, 0.0, 1.0]);
    // Geometry of another component than the path's is refused.
    let error = rig
        .doc
        .query(
            &json!({"query": "joint_frame", "occurrence": "Plate:1",
                    "geometry": rig.pin_bottom()["geometry"]})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("is in Pin, not in Plate"), "{error}");
    // The origin's own geometry in the root.
    let frame = query(&rig.doc, json!({"query": "joint_frame", "geometry": "yz"}));
    assert_near(json_vec(&frame["origin"]), [0.0; 3]);
    assert_near(json_vec(&frame["z_axis"]), [1.0, 0.0, 0.0]);
    // Moved by a joint, the frame follows the occurrence.
    rig.ground(rig.o1, true);
    rig.joint("rigid", json!({"flip": true}));
    let frame = query(
        &rig.doc,
        json!({"query": "joint_frame", "occurrence": "Pin:1",
               "geometry": rig.pin_bottom()["geometry"]}),
    );
    assert_near(json_vec(&frame["origin"]), [5.0, 5.0, 5.0]);
}
