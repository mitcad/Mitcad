// SPDX-License-Identifier: MIT
//! Joints, as-built joints, joint origins and ground items (mitcad#55) on
//! synthetic dumps with the mock kernel: Base (component object 10) is
//! grounded at the origin (occurrence object 100), Arm (20) is placed
//! where the file's joint puts it (200), and occurrence 900 places a
//! component of another document.

use mitcad_model::assembly::inverse;
use mitcad_model::features::{GeomRef, JointDef};
use mitcad_model::joints::{JointKind, Motion};
use mitcad_model::{ComponentUid, Datum, OccurrenceUid, Transform};

use super::*;

const PI: f64 = std::f64::consts::PI;
const DEG30: f64 = PI / 6.0;

fn turn(axis: [f64; 3], angle: f64) -> Transform {
    Transform::rotation([0.0; 3], axis, angle).expect("unit axis")
}

fn shift(v: [f64; 3]) -> Transform {
    Transform::translation(v)
}

/// A placement or frame (mm) as the dump's matrix (cm).
fn cm(t: &Transform) -> Value {
    let row = |r: usize| {
        json!([
            t.linear[r][0],
            t.linear[r][1],
            t.linear[r][2],
            t.translation[r] / 10.0
        ])
    };
    json!([row(0), row(1), row(2), [0.0, 0.0, 0.0, 1.0]])
}

/// A model parameter of the item at `index` with its role (cm, rad).
fn model_param(
    name: &str,
    role: &str,
    expression: &str,
    value: f64,
    unit: &str,
    index: i64,
) -> Value {
    json!({"name": name, "role": role, "expression": expression, "value": value, "unit": unit,
           "createdBy": {"kind": "feature", "objectType": "Joint", "name": "Assemble1",
                         "timeline_index": index}})
}

fn param_ref(name: &str, expression: &str, value: f64, unit: &str) -> Value {
    json!({"kind": "parameter", "name": name, "expression": expression, "value": value,
           "unit": unit})
}

/// An occurrence reference by its path of occurrence objects from the root.
fn occurrence(path: Value) -> Value {
    json!({"kind": "occurrence", "_f3d": {"path": path, "context_component": 3}})
}

/// A `JointGeometry` with a frame (mm) and a key point naming `entity`.
fn joint_geometry(frame: &Transform, entity: Value) -> Value {
    let o = frame.translation.map(|v| v / 10.0);
    let column = |c: usize| json!([frame.linear[0][c], frame.linear[1][c], frame.linear[2][c]]);
    json!({"_type": "JointGeometry", "origin": o, "primaryAxisVector": column(0),
           "secondaryAxisVector": column(1), "thirdAxisVector": column(2),
           "entityOne": entity,
           "_f3d": {"frame": cm(frame), "key_points": [{"point": o, "code": 10, "entity": entity}]}})
}

/// Arm's frame: 10 mm above its origin, z down.
fn frame_one() -> Transform {
    shift([0.0, 0.0, 10.0]).after(&turn([1.0, 0.0, 0.0], PI))
}

/// Base's frame: at (10, 20, 0) mm, turned a quarter about z.
fn frame_two() -> Transform {
    shift([10.0, 20.0, 0.0]).after(&turn([0.0, 0.0, 1.0], PI / 2.0))
}

/// Where the joint's relation `world(one) · frame one = world(two) · frame
/// two · Rz(angle) · Tz(offset) · Rx(π if opposed)` puts Arm (Base at the
/// origin; the file's relation, mitcad#81), the angle turned by `sense`.
fn arm_placement(angle: f64, offset: f64, opposed: bool, sense: f64) -> Transform {
    let over = if opposed {
        turn([1.0, 0.0, 0.0], PI)
    } else {
        Transform::IDENTITY
    };
    let alignment = turn([0.0, 0.0, 1.0], sense * angle)
        .after(&shift([0.0, 0.0, offset]))
        .after(&over);
    frame_two().after(&alignment).after(&inverse(&frame_one()))
}

/// A revolute joint of Arm (side one) onto Base (side two) with an angle of
/// 30°, an offset of 5 mm and rotation limits -90°..90°; the item at
/// `index` names its parameters d11 to d16.
fn revolute(index: i64, opposed: bool, one: Value) -> (Value, Vec<Value>) {
    let p = |n: &str, e: &str, v: f64, u: &str| param_ref(n, e, v, u);
    let motion = json!({"_type": "RevoluteJointMotion", "jointType": "RevoluteJointType",
        "rotationValue": 0.0,
        "rotationLimits": {"_type": "JointLimits",
            "minimumValue": p("d15", "-90 deg", -PI / 2.0, "deg"), "isMinimumValueEnabled": true,
            "maximumValue": p("d16", "90 deg", PI / 2.0, "deg"), "isMaximumValueEnabled": true},
        "_f3d": {"type_code": 1, "motions": [{"motion": "rz", "axis": 2}]}});
    joint(index, opposed, one, motion)
}

/// The joint of [`revolute`] with another motion.
fn joint(index: i64, opposed: bool, one: Value, motion: Value) -> (Value, Vec<Value>) {
    let p = |n: &str, e: &str, v: f64, u: &str| param_ref(n, e, v, u);
    let detail = json!({
        "occurrenceOne": one,
        "occurrenceTwo": occurrence(json!([100])),
        "geometryOrOriginOne": joint_geometry(&frame_one(), json!(null)),
        "geometryOrOriginTwo": joint_geometry(&frame_two(), json!(null)),
        "jointMotion": motion,
        "angle": p("d11", "30 deg", DEG30, "deg"),
        "offset": p("d12", "5 mm", 0.5, "mm"),
        "offsetX": p("d13", "0 mm", 0.0, "mm"),
        "offsetY": p("d14", "0 mm", 0.0, "mm"),
        "isFlipped": !opposed,
        "_f3d": {"opposed": u8::from(opposed), "frames": [cm(&frame_one()), cm(&frame_two())]}
    });
    let item = json!({"index": index, "name": "Assemble1", "objectType": "Joint",
                      "detail": detail, "_f3d": {"component": 3}});
    let params = vec![
        model_param("d11", "alignAngle", "30 deg", DEG30, "deg", index),
        model_param("d12", "alignOffsetZ", "5 mm", 0.5, "mm", index),
        model_param("d13", "alignOffsetX", "0 mm", 0.0, "mm", index),
        model_param("d14", "alignOffsetY", "0 mm", 0.0, "mm", index),
        model_param("d15", "RotateMinimum", "-90 deg", -PI / 2.0, "deg", index),
        model_param("d16", "RotateMaximum", "90 deg", PI / 2.0, "deg", index),
    ];
    (item, params)
}

/// The design with Arm placed at `arm`, the timeline `items` and the
/// model parameters `params`.
fn assembly(arm: &Transform, items: Vec<Value>, params: Vec<Value>) -> Dump {
    let v = json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "joints.f3d"},
        "document": {"root_component": "Assembly"},
        "parameters": {"user": [], "model": params},
        "components": [
            {"name": "Assembly", "is_root": true, "_f3d": {"object_id": 3}},
            {"name": "Base", "_f3d": {"object_id": 10}},
            {"name": "Arm", "_f3d": {"object_id": 20}}],
        "occurrences": [
            {"component": "Base", "transform": cm(&Transform::IDENTITY), "isGrounded": true,
             "_f3d": {"object_id": 100, "component_object": 10}, "children": []},
            {"component": "Arm", "transform": cm(arm),
             "_f3d": {"object_id": 200, "component_object": 20}, "children": []},
            {"component": null, "isReferencedComponent": true, "transform": cm(&Transform::IDENTITY),
             "_f3d": {"object_id": 900, "component_object": 3, "external_key": "00ff"}}],
        "timeline": {"items": items}
    });
    Dump::from_json(&v.to_string()).expect("a dump")
}

fn occurrence_named(doc: &Document<MockKernel>, name: &str) -> OccurrenceUid {
    let a = doc.assembly();
    a.occurrences
        .iter()
        .find(|o| a.occurrence_name(o.uid) == name)
        .map(|o| o.uid)
        .expect(name)
}

/// The placements are still the ones the file stores.
fn assert_unmoved(doc: &Document<MockKernel>) {
    for o in &doc.assembly().occurrences {
        let p = doc.placement(o.uid).unwrap();
        let d = (0..3).fold(0.0_f64, |m, i| {
            m.max((p.translation[i] - o.transform.translation[i]).abs())
        });
        assert!(
            d < 1e-6,
            "{} moved by {d}",
            doc.assembly().occurrence_name(o.uid)
        );
    }
}

fn feature_of(doc: &Document<MockKernel>, report: &DesignReport, k: usize) -> FeatureDef {
    let uid: FeatureUid = report.items[k].features[0].parse().unwrap();
    doc.feature(uid).unwrap().def.clone()
}

fn param_name(doc: &Document<MockKernel>, id: Option<mitcad_model::ParamId>) -> String {
    doc.parameters().name(id.expect("a parameter"))
}

fn joint_query(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(&doc.query(r#"{"query": "joints"}"#).unwrap()).unwrap()
}

#[test]
fn joints_come_in_where_they_hold() {
    let (item, params) = revolute(0, true, occurrence(json!([200])));
    let dump = assembly(&arm_placement(DEG30, 5.0, true, 1.0), vec![item], params);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let j = &report.joints;
    assert_eq!(
        (j.joints, j.fixed_sides, j.kept_as_built, j.skipped),
        (1, 2, 0, 0),
        "{}",
        report.text()
    );
    // No geometry of the mock's components gives the frames: fixed planes.
    assert_eq!(report.items[0].outcome, Outcome::Partial);
    assert!(
        report.items[0]
            .note
            .as_deref()
            .unwrap()
            .contains("side one on a fixed frame"),
        "{}",
        report.text()
    );
    let FeatureDef::Joint(def) = feature_of(&doc, &report, 0) else {
        panic!("a joint: {}", report.text());
    };
    let def: &JointDef = &def;
    assert_eq!(def.kind, JointKind::Revolute);
    assert!(def.flip);
    // The file's parameters: the angle as it is (flipped), the offset, the
    // limits.
    assert_eq!(param_name(&doc, def.angle), "d11");
    assert_eq!(param_name(&doc, def.offset), "d12");
    let rz = &def.limits[&Motion::Rz];
    assert_eq!(param_name(&doc, rz.min), "d15");
    assert_eq!(param_name(&doc, rz.max), "d16");
    assert_eq!(def.a.occurrence.to_string(), "O2");
    assert!(matches!(def.b.geometry, GeomRef::FixedPlane { .. }));
    assert_unmoved(&doc);
    let q = joint_query(&doc);
    assert_eq!(q["joints"][0]["state"], "satisfied", "{q}");
    assert!(q["joints"][0]["values"]["rz"].as_f64().unwrap().abs() < 1e-9);
    assert!(report.warnings.is_empty(), "{}", report.text());
}

#[test]
fn the_angle_is_the_files() {
    // The file's angle as it is, flipped (opposed) or not (mitcad#81: the
    // joint test model's rigid joints at 30°); the other sense does not
    // hold.
    for opposed in [true, false] {
        for (sense, holds) in [(1.0, true), (-1.0, false)] {
            let (item, params) = joint(0, opposed, occurrence(json!([200])), rigid());
            let arm = arm_placement(DEG30, 5.0, opposed, sense);
            let dump = assembly(&arm, vec![item], params);
            let mut doc = Document::new(MockKernel::default());
            let report = import(&mut doc, &dump);
            let j = &report.joints;
            assert_eq!(
                (j.joints, j.kept_as_built),
                (usize::from(holds), usize::from(!holds)),
                "{}",
                report.text()
            );
            if holds {
                let FeatureDef::Joint(def) = feature_of(&doc, &report, 0) else {
                    panic!("a joint");
                };
                assert_eq!(def.flip, opposed);
                assert_eq!(param_name(&doc, def.angle), "d11", "{}", report.text());
            } else {
                // Nothing was tried: the frames do not fit (mitcad#87).
                let note = report.items[0].note.as_deref().unwrap_or_default();
                assert!(note.contains("side one is"), "{note}");
            }
            assert_unmoved(&doc);
        }
    }
}

#[test]
fn limits_hold_the_values_where_the_file_places_the_occurrences() {
    // A revolute joint without an angle, limited to 10°..90° with its rest
    // at 20°, whose turn where the file places Arm is 30° (as is, mitcad#81):
    // the limits as they are, the turn as its position. At -30° they do
    // not hold it: left out.
    let p = |n: &str, e: &str, v: f64, u: &str| param_ref(n, e, v, u);
    let motion = json!({"_type": "RevoluteJointMotion",
        "rotationLimits": {"_type": "JointLimits",
            "minimumValue": p("d15", "10 deg", PI / 18.0, "deg"), "isMinimumValueEnabled": true,
            "maximumValue": p("d16", "90 deg", PI / 2.0, "deg"), "isMaximumValueEnabled": true,
            "restValue": p("d17", "20 deg", PI / 9.0, "deg"), "isRestValueEnabled": true},
        "_f3d": {"type_code": 1, "motions": [{"motion": "rz", "axis": 2}]}});
    for turned in [DEG30, -DEG30] {
        let (mut item, mut params) = joint(0, true, occurrence(json!([200])), motion.clone());
        item["detail"]["angle"] = p("d11", "0 deg", 0.0, "deg");
        params[0] = model_param("d11", "alignAngle", "0 deg", 0.0, "deg", 0);
        params[4] = model_param("d15", "RotateMinimum", "10 deg", PI / 18.0, "deg", 0);
        params.push(model_param(
            "d17",
            "RotateRest",
            "20 deg",
            PI / 9.0,
            "deg",
            0,
        ));
        let arm = arm_placement(turned, 5.0, true, 1.0);
        let dump = assembly(&arm, vec![item], params);
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        assert_eq!(report.joints.joints, 1, "{}", report.text());
        let FeatureDef::Joint(def) = feature_of(&doc, &report, 0) else {
            panic!("a joint");
        };
        let note = report.items[0].note.clone().unwrap_or_default();
        if turned > 0.0 {
            let rz = &def.limits[&Motion::Rz];
            assert_eq!(param_name(&doc, rz.min), "d15");
            assert_eq!(param_name(&doc, rz.max), "d16");
            assert_eq!(param_name(&doc, rz.rest), "d17");
            assert!(def.position.contains_key(&Motion::Rz), "{}", report.text());
            assert!(!note.contains("limits"), "{note}");
        } else {
            assert!(def.limits.is_empty());
            assert!(
                note.contains("its limits left out (outside them: rz is -0.523599 there)"),
                "{note}"
            );
        }
        let q = joint_query(&doc);
        let v = q["joints"][0]["values"]["rz"].as_f64().unwrap();
        assert!((v - turned).abs() < 1e-9, "{q}");
        assert_unmoved(&doc);
    }
}

#[test]
fn joints_that_do_not_hold_are_kept_as_built() {
    // Arm 10 mm off where the joint puts it.
    let (item, params) = revolute(0, true, occurrence(json!([200])));
    let arm = shift([10.0, 0.0, 0.0]).after(&arm_placement(DEG30, 5.0, true, 1.0));
    let dump = assembly(&arm, vec![item], params);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(
        (report.joints.joints, report.joints.kept_as_built),
        (0, 1),
        "{}",
        report.text()
    );
    assert_eq!(report.items[0].outcome, Outcome::Partial);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("Assemble1 does not hold where the file places the occurrences")),
        "{}",
        report.text()
    );
    let FeatureDef::AsBuiltJoint(def) = feature_of(&doc, &report, 0) else {
        panic!("an as-built joint: {}", report.text());
    };
    assert_eq!(def.kind, JointKind::Revolute);
    // The motion frame is side two's.
    assert_eq!(def.origin.as_ref().unwrap().occurrence.to_string(), "O1");
    let r = def.relative.unwrap();
    assert!((r.translation[0] - arm.translation[0]).abs() < 1e-9);
    assert_unmoved(&doc);
}

#[test]
fn as_built_joints_keep_the_stored_placements() {
    let arm = shift([30.0, 0.0, 5.0]).after(&turn([0.0, 0.0, 1.0], 0.3));
    // The joint's frame in each occurrence's component, as the file
    // records it.
    let joint = shift([12.0, 3.0, 4.0]);
    let records = |arm: &Transform| {
        json!([
            {"occurrence": occurrence(json!([200])), "frame": cm(&inverse(arm).after(&joint))},
            {"occurrence": occurrence(json!([100])), "frame": cm(&joint)}])
    };
    let as_built = |index: i64, name: &str, motion: Value, records: Value| {
        json!({"index": index, "name": name, "objectType": "AsBuiltJoint", "_f3d": {"component": 3},
               "detail": {"occurrenceOne": occurrence(json!([200])),
                          "occurrenceTwo": occurrence(json!([100])),
                          "geometry": null, "jointMotion": motion,
                          "_f3d": {"placements": records}}})
    };
    let rigid = json!({"_type": "RigidJointMotion", "_f3d": {"type_code": 11, "values": []}});
    let revolute = json!({"_type": "RevoluteJointMotion", "rotationValue": 0.0,
        "_f3d": {"type_code": 1, "values": [{"value": 0.0, "axis": 2}]}});
    let items = vec![
        as_built(0, "As-built1", rigid.clone(), records(&arm)),
        as_built(1, "As-built2", revolute, records(&arm)),
        // Records of another placement of Arm.
        as_built(
            2,
            "As-built3",
            rigid,
            records(&shift([1.0, 0.0, 0.0]).after(&arm)),
        ),
    ];
    let dump = assembly(&arm, items, Vec::new());
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(report.joints.as_built, 3, "{}", report.text());
    let outcomes: Vec<Outcome> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(outcomes, [Outcome::Parametric; 3]);
    for k in 0..3 {
        let FeatureDef::AsBuiltJoint(def) = feature_of(&doc, &report, k) else {
            panic!("an as-built joint");
        };
        let r = def.relative.unwrap();
        assert!((0..3).all(|i| (r.translation[i] - arm.translation[i]).abs() < 1e-9));
        assert_eq!(def.origin.is_some(), k == 1);
    }
    // The motion frame of the revolute one: the recorded frame on Base.
    let FeatureDef::AsBuiltJoint(def) = feature_of(&doc, &report, 1) else {
        panic!("an as-built joint");
    };
    let origin = def.origin.unwrap();
    assert_eq!(origin.occurrence.to_string(), "O1");
    assert!(
        matches!(origin.geometry, GeomRef::FixedPlane { origin: o, .. } if (o[0] - 12.0).abs() < 1e-9)
    );
    assert_eq!(report.warnings.len(), 1, "{}", report.text());
    assert!(report.warnings[0].starts_with("As-built3: the placement it records differs"));
    assert_unmoved(&doc);
}

#[test]
fn joint_origins_and_ground_items() {
    let origin = json!({"kind": "construction_point", "name": "Origin", "origin": "Origin"});
    let frame = shift([0.0, 0.0, 2.5]);
    let jo_param = |name: &str, role: &str, e: &str, v: f64, u: &str| {
        json!({"name": name, "role": role, "expression": e, "value": v, "unit": u,
               "createdBy": {"kind": "feature", "objectType": "JointOrigin", "name": "JointOrigin1",
                             "timeline_index": 0}})
    };
    let items = vec![
        json!({"index": 0, "name": "JointOrigin1", "objectType": "JointOrigin",
               "_f3d": {"component": 10},
               "detail": {"geometry": joint_geometry(&frame, origin),
                          "offsetX": param_ref("d21", "0 mm", 0.0, "mm"),
                          "offsetY": param_ref("d22", "0 mm", 0.0, "mm"),
                          "offsetZ": param_ref("d23", "2.5 mm", 0.25, "mm"),
                          "angle": param_ref("d24", "0 deg", 0.0, "deg")}}),
        json!({"index": 1, "name": "Ground1", "objectType": "GroundOccurrence",
               "_f3d": {"component": 3},
               "detail": {"occurrence": occurrence(json!([200]))}}),
    ];
    let params = vec![
        jo_param("d21", "OffsetX", "0 mm", 0.0, "mm"),
        jo_param("d22", "OffsetY", "0 mm", 0.0, "mm"),
        jo_param("d23", "OffsetZ", "2.5 mm", 0.25, "mm"),
        jo_param("d24", "AngleZ", "0 deg", 0.0, "deg"),
    ];
    let dump = assembly(&shift([50.0, 0.0, 0.0]), items, params);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let outcomes: Vec<Outcome> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(outcomes, [Outcome::Parametric; 2], "{}", report.text());
    let j = &report.joints;
    assert_eq!((j.origins, j.grounded), (1, 1));
    // On Base's origin point, its z offset the file's parameter.
    let uid: FeatureUid = report.items[0].features[0].parse().unwrap();
    let entry = doc.feature(uid).unwrap();
    assert_eq!(doc.assembly().name(entry.component), "Base");
    let FeatureDef::JointOrigin(def) = &entry.def else {
        panic!("a joint origin");
    };
    assert!(
        matches!(def.geometry, GeomRef::Origin(_)),
        "{:?}",
        def.geometry
    );
    assert_eq!(param_name(&doc, def.offset), "d23");
    let Some(Datum::Plane(p)) = doc.datum(uid) else {
        panic!("a plane");
    };
    assert!((p.origin[2] - 2.5).abs() < 1e-9);
    // Arm is grounded by the ground item.
    let arm = occurrence_named(&doc, "Arm:1");
    assert!(doc.assembly().occurrence(arm).unwrap().grounded);
    assert_eq!(report.items[1].note.as_deref(), Some("grounds Arm:1"));
}

fn rigid() -> Value {
    json!({"_type": "RigidJointMotion", "_f3d": {"type_code": 0, "values": []}})
}

/// A rigid joint of side one, the joint origin of a component of another
/// document (its frame `one` in that component), onto Base's frame two,
/// opposed.
fn inserted_joint(index: i64, name: &str, path: Value, one: &Transform) -> Value {
    json!({"index": index, "name": name, "objectType": "Joint", "_f3d": {"component": 3},
           "detail": {
               "occurrenceOne": occurrence(path),
               "occurrenceTwo": occurrence(json!([100])),
               "geometryOrOriginOne": {"_type": "JointOrigin",
                                       "_f3d": {"entity_input": 50, "target": 60}},
               "geometryOrOriginTwo": joint_geometry(&frame_two(), json!(null)),
               "jointMotion": rigid(), "isFlipped": false,
               "_f3d": {"opposed": 1, "frames": [cm(one), cm(&frame_two())]}}})
}

/// Where the opposed rigid joint of [`inserted_joint`] puts side one's
/// occurrence (Base at the origin).
fn inserted_placement(one: &Transform) -> Transform {
    frame_two()
        .after(&turn([1.0, 0.0, 0.0], PI))
        .after(&inverse(one))
}

/// The design of [`assembly`] with a second occurrence (901) of the
/// component of another document, the two at `first` and `second`.
fn with_inserted(dump: Dump, first: &Transform, second: &Transform) -> Dump {
    let mut v = serde_json::to_value(dump).unwrap();
    let occurrences = v["occurrences"].as_array_mut().unwrap();
    occurrences[2]["transform"] = cm(first);
    let mut other = occurrences[2].clone();
    other["transform"] = cm(second);
    other["_f3d"]["object_id"] = json!(901);
    occurrences.push(other);
    Dump::from_json(&v.to_string()).unwrap()
}

#[test]
fn joints_with_sides_on_components_of_other_documents() {
    // A fastener inserted twice: the first joined on its origin, the
    // second on a frame 2 mm up its z axis; an as-built joint of the two;
    // a joint of an occurrence inside the fastener's own document.
    let up = shift([0.0, 0.0, 2.0]);
    let insert = json!({"index": 0, "name": "Screw1", "objectType": "Fastener",
        "_f3d": {"component": 3},
        "detail": {"occurrence": {"kind": "occurrence", "_f3d": {"path": [900]}}}});
    let as_built = json!({"index": 3, "name": "As-built1", "objectType": "AsBuiltJoint",
        "_f3d": {"component": 3},
        "detail": {"occurrenceOne": occurrence(json!([901])),
                   "occurrenceTwo": occurrence(json!([900])), "geometry": null,
                   "jointMotion": {"_type": "RigidJointMotion",
                                   "_f3d": {"type_code": 11, "values": []}}}});
    let items = vec![
        insert,
        inserted_joint(1, "Assemble1", json!([900]), &Transform::IDENTITY),
        inserted_joint(2, "Assemble2", json!([901]), &up),
        as_built,
        inserted_joint(4, "Assemble3", json!([900, null]), &Transform::IDENTITY),
    ];
    let dump = assembly(&Transform::IDENTITY, items, Vec::new());
    let dump = with_inserted(
        dump,
        &inserted_placement(&Transform::IDENTITY),
        &inserted_placement(&up),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let j = &report.joints;
    assert_eq!(
        (j.joints, j.inserted_sides, j.as_built, j.skipped),
        (2, 2, 1, 1),
        "{}",
        report.text()
    );
    assert_eq!(report.components.external, 2);
    // One empty component, named after the item that inserted it.
    let screw = doc
        .assembly()
        .components
        .iter()
        .find(|c| c.name == "Screw1")
        .map(|c| c.uid)
        .expect("Screw1");
    assert!(doc.component_bodies(screw).is_empty());
    let first = occurrence_named(&doc, "Screw1:1");
    let second = occurrence_named(&doc, "Screw1:2");
    for (k, occurrence, origin) in [(1, first, None), (2, second, Some([0.0, 0.0, 2.0]))] {
        let FeatureDef::Joint(def) = feature_of(&doc, &report, k) else {
            panic!("a joint: {}", report.text());
        };
        assert_eq!(def.a.occurrence.to_string(), occurrence.to_string());
        assert!(
            matches!(def.a.geometry, GeomRef::Origin(_)),
            "{:?}",
            def.a.geometry
        );
        assert_eq!(def.a.frame_override.and_then(|o| o.origin), origin);
        assert!(def.flip);
        assert!(
            report.items[k]
                .note
                .as_deref()
                .unwrap()
                .contains("side one on the origin of a component of another document"),
            "{}",
            report.text()
        );
    }
    let note = report.items[4].note.clone().unwrap_or_default();
    assert!(
        note.contains("inside a component of another document"),
        "{note}"
    );
    assert_unmoved(&doc);
}

/// Captured positions (mitcad#75): Arm is stored at the origin, a joint
/// before the first captured position and one after it hold only where
/// the last captured position puts Arm, the first puts it elsewhere.
#[test]
fn captured_positions_come_in_and_place_the_joints() {
    let at = arm_placement(DEG30, 5.0, true, 1.0);
    let elsewhere = shift([0.0, 40.0, 0.0]);
    let position = |index: i64, name: &str, t: &Transform| {
        json!({"index": index, "name": name, "objectType": "Snapshot", "_f3d": {"component": 3},
               "detail": {"positions": [{"occurrence": occurrence(json!([200])),
                                         "transform": cm(t)}]}})
    };
    // Rigid joints: Arm has one place where they hold.
    let (before, mut params) = joint(0, true, occurrence(json!([200])), rigid());
    let (mut after, more) = joint(3, true, occurrence(json!([200])), rigid());
    after["name"] = json!("Assemble2");
    // The second joint's own parameters.
    params.extend(more.into_iter().map(|mut p| {
        let name = p["name"].as_str().unwrap().replace('d', "e");
        p["name"] = json!(name);
        p
    }));
    for slot in ["angle", "offset", "offsetX", "offsetY"] {
        let name = after["detail"][slot]["name"]
            .as_str()
            .unwrap()
            .replace('d', "e");
        after["detail"][slot]["name"] = json!(name);
    }
    let items = vec![
        before,
        position(1, "Position1", &elsewhere),
        position(2, "Position2", &at),
        after,
    ];
    let dump = assembly(&Transform::IDENTITY, items, params);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let j = &report.joints;
    assert_eq!(
        (j.joints, j.positions, j.kept_as_built),
        (2, 2, 0),
        "{}",
        report.text()
    );
    let arm = occurrence_named(&doc, "Arm:1");
    // Arm starts at its last captured position (its stored transform is
    // older), and the captured positions put it where the joints hold.
    let own = doc.assembly().occurrence(arm).unwrap().transform;
    let placed = doc.placement(arm).unwrap();
    for t in [own, placed] {
        assert!(
            (0..3).all(|i| (t.translation[i] - at.translation[i]).abs() < 1e-9),
            "{t:?}"
        );
    }
    let uid: FeatureUid = report.items[2].features[0].parse().unwrap();
    assert!(matches!(
        &doc.feature(uid).unwrap().def,
        FeatureDef::CapturePosition(_)
    ));
    let q = joint_query(&doc);
    assert_eq!(q["joints"][1]["state"], "satisfied", "{q}");
}

/// An as-built joint before a captured position takes the placements the
/// file ends with (its records agree with them), not Arm's stored one.
#[test]
fn as_built_joints_before_captured_positions() {
    let at = shift([30.0, 0.0, 5.0]);
    let joint = shift([12.0, 3.0, 4.0]);
    let records = json!([
        {"occurrence": occurrence(json!([200])), "frame": cm(&inverse(&at).after(&joint))},
        {"occurrence": occurrence(json!([100])), "frame": cm(&joint)}]);
    let items = vec![
        json!({"index": 0, "name": "As-built1", "objectType": "AsBuiltJoint",
               "_f3d": {"component": 3},
               "detail": {"occurrenceOne": occurrence(json!([200])),
                          "occurrenceTwo": occurrence(json!([100])), "geometry": null,
                          "jointMotion": {"_type": "RigidJointMotion",
                                          "_f3d": {"type_code": 11, "values": []}},
                          "_f3d": {"placements": records}}}),
        json!({"index": 1, "name": "Position1", "objectType": "Snapshot",
               "_f3d": {"component": 3},
               "detail": {"positions": [{"occurrence": occurrence(json!([200])),
                                         "transform": cm(&at)}]}}),
    ];
    let dump = assembly(&Transform::IDENTITY, items, Vec::new());
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let j = &report.joints;
    assert_eq!((j.as_built, j.positions), (1, 1), "{}", report.text());
    assert!(report.warnings.is_empty(), "{}", report.text());
    let FeatureDef::AsBuiltJoint(def) = feature_of(&doc, &report, 0) else {
        panic!("an as-built joint");
    };
    let r = def.relative.unwrap();
    assert!((0..3).all(|i| (r.translation[i] - at.translation[i]).abs() < 1e-9));
    let arm = doc.placement(occurrence_named(&doc, "Arm:1")).unwrap();
    assert!((0..3).all(|i| (arm.translation[i] - at.translation[i]).abs() < 1e-9));
}

/// A captured position names a sub-assembly by an id the occurrence tree
/// does not have; the position of the part inside it tells which
/// occurrence it is.
#[test]
fn captured_positions_find_unknown_levels() {
    let sub = shift([0.0, 0.0, 30.0]);
    let part = shift([5.0, 0.0, 0.0]);
    let v = json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "captured.f3d"},
        "document": {"root_component": "Assembly"},
        "components": [
            {"name": "Assembly", "is_root": true, "_f3d": {"object_id": 3}},
            {"name": "Sub", "_f3d": {"object_id": 30}},
            {"name": "Part", "_f3d": {"object_id": 40}}],
        "occurrences": [
            {"component": "Sub", "transform": cm(&Transform::IDENTITY),
             "_f3d": {"object_id": 300, "component_object": 30},
             "children": [
                 {"component": "Part", "_f3d": {"object_id": 400, "component_object": 40,
                                                 "local_transform": cm(&Transform::IDENTITY)},
                  "children": []}]}],
        "timeline": {"items": [
            {"index": 0, "name": "Position1", "objectType": "Snapshot", "_f3d": {"component": 3},
             "detail": {"positions": [
                 {"occurrence": {"kind": "occurrence",
                                 "_f3d": {"path": [null], "path_guids": ["old"],
                                          "context_component": 3}},
                  "transform": cm(&sub)},
                 {"occurrence": {"kind": "occurrence",
                                 "_f3d": {"path": [null, 400], "path_guids": ["old", "part"],
                                          "context_component": 3}},
                  "transform": cm(&sub.after(&part))}]}}]}
    });
    let dump = Dump::from_json(&v.to_string()).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(report.joints.positions, 1, "{}", report.text());
    assert_eq!(report.items[0].outcome, Outcome::Parametric);
    // One feature in the root (Sub) and one in Sub (Part).
    assert_eq!(report.items[0].features.len(), 2, "{}", report.text());
    let at = |name: &str| doc.placement(occurrence_named(&doc, name)).unwrap();
    assert!((at("Sub:1").translation[2] - 30.0).abs() < 1e-9);
    assert!((at("Part:1").translation[0] - 5.0).abs() < 1e-9);
    assert!(at("Part:1").translation[2].abs() < 1e-9);
}

#[test]
fn joint_sides_on_faces_of_the_replay() {
    // Base's block (40 x 20 x 10 mm), and side two's frame on its top face
    // at (10, 10, 10) mm: the face gives the plane, the origin is
    // overridden.
    let mut block = serde_json::to_value(block_dump()).unwrap();
    let items = block["timeline"]["items"].as_array_mut().unwrap();
    for item in items.iter_mut() {
        item["_f3d"]["component"] = json!(10);
    }
    let top = shift([10.0, 10.0, 10.0]);
    let (mut joint, mut params) = revolute(2, true, occurrence(json!([200])));
    joint["detail"]["geometryOrOriginTwo"] = joint_geometry(&top, json!({"kind": "face"}));
    joint["detail"]["_f3d"]["frames"][1] = cm(&top);
    let alignment = turn([0.0, 0.0, 1.0], DEG30)
        .after(&turn([1.0, 0.0, 0.0], PI))
        .after(&shift([0.0, 0.0, -5.0]));
    let arm = top
        .after(&inverse(&alignment))
        .after(&inverse(&frame_one()));
    let mut all = items.clone();
    all.push(joint);
    params.extend(
        block["parameters"]["model"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    let mut dump: Value = serde_json::to_value(assembly(&arm, all, params)).unwrap();
    dump["parameters"]["user"] = block["parameters"]["user"].clone();
    let dump = Dump::from_json(&dump.to_string()).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(
        (report.joints.joints, report.joints.fixed_sides),
        (1, 1),
        "{}",
        report.text()
    );
    let FeatureDef::Joint(def) = feature_of(&doc, &report, 2) else {
        panic!("a joint");
    };
    let GeomRef::Face { body, face } = &def.b.geometry else {
        panic!("a face: {:?}", def.b.geometry);
    };
    assert_eq!(face.role, "end");
    assert_eq!(
        doc.body_component(*body),
        doc.assembly()
            .components
            .iter()
            .find(|c| c.name == "Base")
            .map(|c| c.uid)
    );
    let o = def.b.frame_override.unwrap();
    assert_eq!(o.origin, Some([10.0, 10.0, 10.0]));
    assert_eq!(o.z_axis, None);
    assert_ne!(doc.body_component(*body), Some(ComponentUid::ROOT));
    assert_unmoved(&doc);
}

/// The dump of [`assembly`] with Arm's placements (mitcad#81): made by the
/// item at `made` at its stored transform, then put at each `(index,
/// placement)` by the items there.
fn with_placements(mut dump: Dump, made: i64, steps: &[(i64, Transform)]) -> Dump {
    let mut v = serde_json::to_value(&dump).unwrap();
    let stored = v["occurrences"][1]["transform"].clone();
    let mut list = vec![json!({"index": made, "object_id": 1, "transform": stored})];
    list.extend(
        steps
            .iter()
            .map(|(i, t)| json!({"index": i, "object_id": 2, "transform": cm(t)})),
    );
    v["occurrences"][1]["_f3d"]["placements"] = json!(list);
    dump = Dump::from_json(&v.to_string()).expect("a dump");
    dump
}

/// Joints move the occurrences from the transforms the file stores
/// (mitcad#81): the placements after each item say where; a joint holds
/// there, and Arm starts at its last one.
#[test]
fn joints_hold_where_the_placements_after_them_say() {
    let at = arm_placement(DEG30, 5.0, true, 1.0);
    let stored = shift([100.0, 100.0, 40.0]);
    let (item, params) = joint(1, true, occurrence(json!([200])), rigid());
    let dump = with_placements(assembly(&stored, vec![item], params), 0, &[(1, at)]);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(
        (report.joints.joints, report.joints.kept_as_built),
        (1, 0),
        "{}",
        report.text()
    );
    let arm = doc.placement(occurrence_named(&doc, "Arm:1")).unwrap();
    assert!((0..3).all(|i| (arm.translation[i] - at.translation[i]).abs() < 1e-9));
    // Without them the stored transform is where the file places Arm:
    // nothing holds the joint there.
    let (item, params) = joint(1, true, occurrence(json!([200])), rigid());
    let dump = assembly(&stored, vec![item], params);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(report.joints.kept_as_built, 1, "{}", report.text());
}

/// A captured position whose own positions are not decoded (that of
/// joints' values) places the occurrences its placements name.
#[test]
fn captured_positions_from_the_placements() {
    let at = arm_placement(DEG30, 5.0, true, 1.0);
    let items = vec![
        json!({"index": 1, "name": "Position1", "objectType": "Snapshot",
                            "_f3d": {"component": 3}, "detail": {}}),
    ];
    let dump = with_placements(
        assembly(&Transform::IDENTITY, items, Vec::new()),
        0,
        &[(1, at)],
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(report.joints.positions, 1, "{}", report.text());
    let uid: FeatureUid = report.items[0].features[0].parse().unwrap();
    let FeatureDef::CapturePosition(def) = &doc.feature(uid).unwrap().def else {
        panic!("a captured position");
    };
    assert_eq!(def.positions.len(), 1);
    let arm = doc.placement(occurrence_named(&doc, "Arm:1")).unwrap();
    assert!((0..3).all(|i| (arm.translation[i] - at.translation[i]).abs() < 1e-9));
    // A position with an empty path: the placements name its occurrence.
    let items = vec![
        json!({"index": 1, "name": "Position1", "objectType": "Snapshot",
        "_f3d": {"component": 3},
        "detail": {"positions": [{"occurrence": occurrence(json!([])), "transform": cm(&at)}]}}),
    ];
    let dump = with_placements(
        assembly(&Transform::IDENTITY, items, Vec::new()),
        0,
        &[(1, at)],
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(report.joints.positions, 1, "{}", report.text());
    assert_eq!(
        report.items[0].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
}

/// A rigid group (mitcad#81) of Arm and the occurrence of another
/// document; a member inside another follows it.
#[test]
fn rigid_groups_come_in() {
    let item = json!({"index": 0, "name": "RigidGroup1", "objectType": "RigidGroup",
                      "_f3d": {"component": 3},
                      "detail": {"occurrences": [occurrence(json!([200])),
                                                 occurrence(json!([900])),
                                                 occurrence(json!([200, 200]))]}});
    let dump = assembly(&shift([50.0, 0.0, 0.0]), vec![item], Vec::new());
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(report.joints.rigid_groups, 1, "{}", report.text());
    let FeatureDef::RigidGroup(def) = feature_of(&doc, &report, 0) else {
        panic!("a rigid group: {}", report.text());
    };
    assert_eq!(def.occurrences.len(), 2);
    assert_eq!(
        report.items[0].note.as_deref(),
        Some("1 of its members below the top level move with the occurrences they are in")
    );
    assert_unmoved(&doc);
    // With the component's own geometry: Arm joined to it.
    let item = json!({"index": 0, "name": "RigidGroup1", "objectType": "RigidGroup",
                      "_f3d": {"component": 3},
                      "detail": {"occurrences": [occurrence(json!([200])),
                                                 occurrence(json!([]))]}});
    let dump = assembly(&shift([50.0, 0.0, 0.0]), vec![item], Vec::new());
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(report.joints.rigid_groups, 1, "{}", report.text());
    let FeatureDef::AsBuiltJoint(def) = feature_of(&doc, &report, 0) else {
        panic!("an as-built joint: {}", report.text());
    };
    assert_eq!(def.kind, JointKind::Rigid);
    assert!(def.b.0.is_empty());
    assert_unmoved(&doc);
}

/// An offset the file stores rounded (mitcad#81, the owner's decision):
/// within the rounding of its expression the placements give it, and its
/// parameter keeps its name with that value; beyond it the joint is kept
/// as an as-built joint.
#[test]
fn rounded_offsets_come_from_the_placements() {
    for (offset, holds) in [(5.003, true), (5.02, false)] {
        let (mut item, mut params) = joint(0, true, occurrence(json!([200])), rigid());
        item["detail"]["offset"] = param_ref("d12", "5.00 mm", 0.5, "mm");
        params[1] = model_param("d12", "alignOffsetZ", "5.00 mm", 0.5, "mm", 0);
        let dump = assembly(&arm_placement(DEG30, offset, true, 1.0), vec![item], params);
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        assert_eq!(
            report.joints.joints,
            usize::from(holds),
            "{}",
            report.text()
        );
        if holds {
            let FeatureDef::Joint(def) = feature_of(&doc, &report, 0) else {
                panic!("a joint");
            };
            assert_eq!(param_name(&doc, def.offset), "d12");
            let value = doc.parameters().value(def.offset.unwrap()).unwrap();
            assert!((value - offset).abs() < 1e-9, "{value}");
            let note = report.items[0].note.clone().unwrap_or_default();
            assert!(
                note.contains(
                    "its offset 5.003000 mm from the placements (the file stores 5.00 mm)"
                ),
                "{note}"
            );
        } else {
            // The parameter keeps the file's value.
            let d12 = doc.parameters().find("d12").unwrap();
            assert!((doc.parameters().value(d12).unwrap() - 5.0).abs() < 1e-12);
        }
        assert_unmoved(&doc);
    }
}

/// An as-built joint's limits hold at its recorded placement, its value
/// 0 there: the rest value elsewhere gives it a position of 0.
#[test]
fn as_built_limits_hold_at_the_recorded_placement() {
    let arm = shift([30.0, 0.0, 5.0]);
    let joint = shift([12.0, 3.0, 4.0]);
    let records = json!([
        {"occurrence": occurrence(json!([200])), "frame": cm(&inverse(&arm).after(&joint))},
        {"occurrence": occurrence(json!([100])), "frame": cm(&joint)}]);
    let p = |n: &str, e: &str, v: f64, u: &str| param_ref(n, e, v, u);
    let motion = json!({"_type": "RevoluteJointMotion",
        "rotationLimits": {"_type": "JointLimits",
            "minimumValue": p("d15", "-10 deg", -PI / 18.0, "deg"), "isMinimumValueEnabled": true,
            "maximumValue": p("d16", "90 deg", PI / 2.0, "deg"), "isMaximumValueEnabled": true,
            "restValue": p("d17", "20 deg", PI / 9.0, "deg"), "isRestValueEnabled": true},
        "_f3d": {"type_code": 1, "motions": [{"motion": "rz", "axis": 2}]}});
    let item = json!({"index": 0, "name": "As-built1", "objectType": "AsBuiltJoint",
                      "_f3d": {"component": 3},
                      "detail": {"occurrenceOne": occurrence(json!([200])),
                                 "occurrenceTwo": occurrence(json!([100])),
                                 "geometry": null, "jointMotion": motion,
                                 "_f3d": {"placements": records}}});
    let params = vec![
        model_param("d15", "RotateMinimum", "-10 deg", -PI / 18.0, "deg", 0),
        model_param("d16", "RotateMaximum", "90 deg", PI / 2.0, "deg", 0),
        model_param("d17", "RotateRest", "20 deg", PI / 9.0, "deg", 0),
    ];
    let dump = assembly(&arm, vec![item], params);
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(report.joints.as_built, 1, "{}", report.text());
    let FeatureDef::AsBuiltJoint(def) = feature_of(&doc, &report, 0) else {
        panic!("an as-built joint");
    };
    assert_eq!(param_name(&doc, def.limits[&Motion::Rz].rest), "d17");
    assert!(def.position.contains_key(&Motion::Rz));
    assert_unmoved(&doc);
}

/// The joint test model (mitcad#81; `MITCAD_F3D_MODELS`, default
/// `~/f3d-models`, kept outside the repository: `joint_kinds/joint_kinds.f3d`
/// with the values read back from the design, `result.json`): every joint
/// kind comes in as a joint at its values (the senses as the file has
/// them), the rigid groups and captured positions come in.
#[test]
fn the_joint_test_model() {
    let dir = match std::env::var_os("MITCAD_F3D_MODELS") {
        Some(d) => std::path::PathBuf::from(d),
        None => match std::env::var_os("HOME") {
            Some(h) => std::path::PathBuf::from(h).join("f3d-models"),
            None => return,
        },
    }
    .join("joint_kinds");
    let (f3d, result) = (dir.join("joint_kinds.f3d"), dir.join("result.json"));
    if !f3d.is_file() || !result.is_file() {
        eprintln!("the joint test model was not found; skipped");
        return;
    }
    let designs = mitcad_f3d::design::decode_path(&f3d).expect("decoded");
    let dump = designs.into_iter().find_map(|d| d.dump).expect("a design");
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    let j = &report.joints;
    assert_eq!(
        (
            j.joints,
            j.kept_as_built,
            j.as_built,
            j.rigid_groups,
            j.positions,
            j.skipped
        ),
        (12, 0, 1, 2, 2, 0),
        "{}",
        report.text()
    );
    // The values the design reads back (cm and rad) by the occurrence its
    // joint moves.
    let truth: Value =
        serde_json::from_str(&std::fs::read_to_string(&result).unwrap()).expect("JSON");
    let truth = &truth["result"]["value"]["joints"];
    let q = joint_query(&doc);
    let mut checked = 0;
    for joint in q["joints"].as_array().unwrap() {
        let occurrence = joint["a"]["occurrence"].as_str().unwrap_or_default();
        let Some((_, read)) = truth
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, t)| t["occurrenceOne"] == occurrence)
        else {
            continue;
        };
        let values = &joint["values"];
        let expect = |motion: &str, key: &str, scale: f64| {
            let want = read[key].as_f64().unwrap() * scale;
            let got = values[motion].as_f64().unwrap_or(f64::NAN);
            assert!(
                (got - want).abs() < 1e-6,
                "{occurrence} {motion}: {got} against {want}: {q}"
            );
        };
        match joint["kind"].as_str().unwrap() {
            "revolute" => expect("rz", "rotationValue", 1.0),
            "slider" => {
                let motion = ["tx", "ty", "tz"]
                    .into_iter()
                    .find(|m| values.get(m).is_some())
                    .unwrap();
                expect(motion, "slideValue", 10.0);
            }
            "cylindrical" | "pin_slot" => {
                expect("rz", "rotationValue", 1.0);
                let slide = if joint["kind"] == "pin_slot" {
                    "tx"
                } else {
                    "tz"
                };
                expect(slide, "slideValue", 10.0);
            }
            "planar" => {
                expect("rz", "rotationValue", 1.0);
                expect("tx", "primarySlideValue", 10.0);
                expect("ty", "secondarySlideValue", 10.0);
            }
            "ball" => {
                // Rz(pitch) · Rx(yaw) · Rz(roll) is Mitcad's Rz · Ry · Rx
                // at other values: compare the turns.
                let ours = turn([0.0, 0.0, 1.0], values["rz"].as_f64().unwrap())
                    .after(&turn([0.0, 1.0, 0.0], values["ry"].as_f64().unwrap()))
                    .after(&turn([1.0, 0.0, 0.0], values["rx"].as_f64().unwrap()));
                let file = turn([0.0, 0.0, 1.0], read["pitchValue"].as_f64().unwrap())
                    .after(&turn([1.0, 0.0, 0.0], read["yawValue"].as_f64().unwrap()))
                    .after(&turn([0.0, 0.0, 1.0], read["rollValue"].as_f64().unwrap()));
                let differ = (0..3)
                    .flat_map(|r| (0..3).map(move |c| (r, c)))
                    .any(|(r, c)| (ours.linear[r][c] - file.linear[r][c]).abs() > 1e-9);
                assert!(!differ, "{occurrence}: {ours:?} against {file:?}");
            }
            _ => {}
        }
        checked += 1;
    }
    assert_eq!(checked, 12, "{q}");
}
