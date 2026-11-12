// SPDX-License-Identifier: MIT
//! Animate Joint's frames without a recompute (mitcad#1701): the
//! `joint_poses` query on a four-bar linkage with the mock kernel. Each
//! frame puts the links where Drive Joint puts them, from the joint solver
//! alone, and changes nothing of the document.
//!
//! Links are components without bodies: their joints are on fixed points
//! of their coordinates (a frame along the component's axes).

use std::f64::consts::{FRAC_PI_2, FRAC_PI_3, PI};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::Vec3;
use mitcad_model::ids::{FeatureUid, OccurrenceUid};
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

fn turn(angle: f64) -> Transform {
    Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], angle).unwrap()
}

fn assert_near(a: Vec3, b: Vec3, what: &str) {
    assert!(
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6),
        "{what}: {a:?} != {b:?}"
    );
}

fn link(doc: &mut Document<MockKernel>, name: &str, at: Transform) -> OccurrenceUid {
    let (_, o) = doc.create_component(Some(name), at, false).unwrap();
    o
}

fn pin(occurrence: &str, p: Vec3) -> Value {
    json!({"occurrence": occurrence, "geometry": {"point": p}})
}

fn ground(p: Vec3) -> Value {
    json!({"geometry": {"point": p}})
}

/// Ground pivots at the origin and 100 along x, a crank of 30, a coupler of
/// 100 and a rocker of 60 (as the joint solver tests' four-bar linkage).
fn four_bar() -> (Document<MockKernel>, [OccurrenceUid; 3], Vec<FeatureUid>) {
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
        let result = run(
            &mut doc,
            json!({"cmd": "add_joint", "kind": "revolute", "a": a, "b": b}),
        );
        assert_eq!(result["error"], Value::Null, "{result}");
        joints.push(result["uid"].as_str().unwrap().parse().unwrap());
    }
    (doc, [crank, coupler, rocker], joints)
}

/// A frame's placement of an occurrence (a path of one uid from the root)
/// applied to a point of its component; None when the frame leaves it
/// where the document has it.
fn placed(frame: &Value, o: OccurrenceUid, p: Vec3) -> Option<Vec3> {
    let rows = frame["placements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["path"] == o.to_string())?["transform"]
        .clone();
    let m: Vec<Vec<f64>> = serde_json::from_value(rows).unwrap();
    Some(std::array::from_fn(|i| {
        m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3]
    }))
}

#[test]
fn frames_put_the_links_where_drive_joint_puts_them() {
    let (mut doc, [crank, coupler, rocker], joints) = four_bar();
    let points = [
        (crank, [30.0, 0.0, 0.0]),
        (coupler, [0.0, 0.0, 0.0]),
        (coupler, [100.0, 0.0, 0.0]),
        (rocker, [0.0, 0.0, 0.0]),
    ];
    let world = |doc: &Document<MockKernel>, o: OccurrenceUid, p: Vec3| {
        doc.placement(o).unwrap().apply_point(p)
    };
    let before: Vec<Vec3> = points.iter().map(|(o, p)| world(&doc, *o, *p)).collect();
    let undo = doc.undo_label().map(str::to_owned);

    // A turn of the crank in steps of 30 degrees, from where it is.
    let start = query(&doc, json!({"query": "joints"}))["joints"][0]["values"]["rz"]
        .as_f64()
        .unwrap();
    let values: Vec<f64> = (1..=12_i32)
        .map(|i| start + PI / 6.0 * f64::from(i))
        .collect();
    let answer = query(
        &doc,
        json!({"query": "joint_poses", "joint": "Joint1", "motion": "rz", "values": values}),
    );
    let frames = answer["frames"].as_array().unwrap();
    assert_eq!(frames.len(), values.len(), "{answer}");

    // Nothing of the document changed: no undo step, the links where they
    // were (the query takes the document read-only, so it cannot recompute).
    assert_eq!(doc.undo_label().map(str::to_owned), undo);
    for ((o, p), b) in points.iter().zip(&before) {
        assert_near(world(&doc, *o, *p), *b, "the document's links stayed");
    }

    // Every frame: the crank's pin where the value turns it, the joints'
    // values, and the links where Drive Joint puts them.
    for (frame, value) in frames.iter().zip(&values) {
        let at = placed(frame, crank, [30.0, 0.0, 0.0]).unwrap_or(before[0]);
        assert_near(
            at,
            [30.0 * value.cos(), 30.0 * value.sin(), 0.0],
            "the crank's pin",
        );
        let reported = frame["values"][joints[0].to_string()]["rz"]
            .as_f64()
            .unwrap();
        let wrapped = (reported - value).rem_euclid(2.0 * PI);
        assert!(
            wrapped < 1e-6 || 2.0 * PI - wrapped < 1e-6,
            "{reported} != {value}"
        );
        // The loop still closes: the coupler between the crank's and the
        // rocker's pins, the rocker on its ground pivot.
        let coupler_a = placed(frame, coupler, [0.0; 3]).unwrap_or(before[1]);
        let coupler_b = placed(frame, coupler, [100.0, 0.0, 0.0]).unwrap_or(before[2]);
        assert_near(coupler_a, at, "the coupler on the crank");
        let rocker_a = placed(frame, rocker, [0.0; 3]).unwrap_or(before[3]);
        assert_near(coupler_b, rocker_a, "the rocker on the coupler");
        let rocker_b = placed(frame, rocker, [60.0, 0.0, 0.0])
            .unwrap_or_else(|| world(&doc, rocker, [60.0, 0.0, 0.0]));
        assert_near(rocker_b, [100.0, 0.0, 0.0], "the rocker on its pivot");
    }

    // A frame at 60 degrees as Drive Joint recomputes it.
    let answer = query(
        &doc,
        json!({"query": "joint_poses", "joint": "Joint1", "motion": "rz",
               "values": [FRAC_PI_3]}),
    );
    let frame = &answer["frames"][0];
    run(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"rz": "60 deg"}}),
    );
    for (o, p) in points {
        let shown = placed(frame, o, p).unwrap_or_else(|| world(&doc, o, p));
        assert_near(shown, world(&doc, o, p), "as Drive Joint places it");
    }
}

#[test]
fn limits_hold_and_wrong_input_is_refused() {
    let (mut doc, [crank, ..], joints) = four_bar();
    let uid = joints[0];
    let mut def = query(&doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    def["limits"] = json!({"rz": {"min": -PI / 6.0, "max": PI / 6.0}});
    run(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid, "def": def}),
    );
    // 90 degrees asked: held at the limit, 30.
    let answer = query(
        &doc,
        json!({"query": "joint_poses", "joint": "Joint1", "motion": "rz", "values": [FRAC_PI_2]}),
    );
    let at = placed(&answer["frames"][0], crank, [30.0, 0.0, 0.0]).unwrap();
    let limit = PI / 6.0;
    assert_near(
        at,
        [30.0 * limit.cos(), 30.0 * limit.sin(), 0.0],
        "held at the limit",
    );

    let refused =
        |doc: &Document<MockKernel>, q: Value| doc.query(&q.to_string()).unwrap_err().to_string();
    let error = refused(
        &doc,
        json!({"query": "joint_poses", "joint": "Joint1", "motion": "tz", "values": [1]}),
    );
    assert!(error.contains("no free motion tz"), "{error}");
    let error = refused(
        &doc,
        json!({"query": "joint_poses", "joint": "Crank", "motion": "rz", "values": [1]}),
    );
    assert!(error.contains("no joint Crank"), "{error}");
    // A suppressed joint is not in effect.
    run(&mut doc, json!({"cmd": "suppress_feature", "uid": uid}));
    let error = refused(
        &doc,
        json!({"query": "joint_poses", "joint": "Joint1", "motion": "rz", "values": [1]}),
    );
    assert!(error.contains("not in effect"), "{error}");
}
