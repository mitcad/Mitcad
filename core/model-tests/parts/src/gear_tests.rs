// SPDX-License-Identifier: MIT
//! Gears (mitcad#472) in documents with the mock kernel: parameters as
//! expressions, rebuilds, saved designs, checks and meshes.

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, ModelError};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> Result<FeatureUid, ModelError> {
    doc.add_feature(&def(value), None).map(|added| added.uid)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn specs(doc: &Document<MockKernel>) -> Vec<String> {
    doc.kernel().specs.borrow().clone()
}

fn bounds(doc: &Document<MockKernel>, body: &str) -> ([f64; 3], [f64; 3]) {
    let uid: BodyUid = body.parse().unwrap();
    let shape = doc.body_shape(uid).expect("the body");
    let b = shape.bounds.expect("bounds");
    (b.min, b.max)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1.0e-6
}

fn rejected(doc: &mut Document<MockKernel>, value: Value) -> String {
    match add(doc, value) {
        Err(error) => error.to_string(),
        Ok(uid) => match status(doc, uid) {
            FeatureStatus::Failed(message) => message.agent_text().to_owned(),
            other => panic!("expected a failure, got {other:?}"),
        },
    }
}

#[test]
fn gears_take_expressions_rebuild_and_reopen() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "gm", "expression": "2 mm"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "z", "expression": "20", "unit": ""}),
    );
    let gear = add(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "plane": "xy", "module": "gm", "teeth": "z",
               "face_width": "gm * 5", "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, gear), FeatureStatus::Ok);
    assert!(
        specs(&doc)
            .last()
            .unwrap()
            .ends_with("from 0 length 10 twist 0"),
        "{:?}",
        specs(&doc)
    );
    // The outside diameter m (z + 2).
    let (min, max) = bounds(&doc, "F1.b0");
    assert!(near(max[0], 22.0) && near(min[0], -22.0), "{min:?} {max:?}");
    assert!(near(max[2], 10.0));

    // A new module rebuilds the gear.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "gm", "expression": "3 mm"}),
    );
    assert_eq!(status(&doc, gear), FeatureStatus::Ok);
    let (min, max) = bounds(&doc, "F1.b0");
    assert!(near(max[0], 33.0) && near(min[0], -33.0), "{min:?} {max:?}");
    assert!(near(max[2], 15.0));

    // Saved and opened again: the same definition and gear.
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(status(&loaded, gear), FeatureStatus::Ok);
    assert_eq!(
        loaded.feature(gear).unwrap().def,
        doc.feature(gear).unwrap().def
    );
    let (min, max) = bounds(&loaded, "F1.b0");
    assert!(near(max[0], 33.0) && near(min[0], -33.0));
    assert!(near(max[2], 15.0));
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert!(saved.contains("\"module\": \"gm\"") || file.to_string().contains("\"module\":\"gm\""));
}

#[test]
fn gears_check_their_values() {
    let mut doc = Document::new(MockKernel::default());
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "helical", "plane": "xy", "module": 2, "teeth": 20,
               "face_width": 10, "operation": "new_body"}),
    );
    assert!(message.contains("needs a helix angle"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "plane": "xy", "module": 2, "teeth": 20,
               "face_width": 10, "helix_angle": 0.3, "operation": "new_body"}),
    );
    assert!(message.contains("has no helix angle"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "plane": "xy", "module": 2, "teeth": 20.5,
               "face_width": 10, "operation": "new_body"}),
    );
    assert!(message.contains("must be whole"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "plane": "xy", "module": 2, "teeth": 20,
               "face_width": 10, "bore": {"type": "plain", "diameter": 36},
               "operation": "new_body"}),
    );
    assert!(message.contains("the bore reaches the teeth"), "{message}");
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "ring", "plane": "xy", "module": 2, "teeth": 40,
               "face_width": 10, "operation": "new_body"}),
    );
    assert!(message.contains("needs a rim thickness"), "{message}");
}

#[test]
fn bores_hubs_and_keyways() {
    let mut doc = Document::new(MockKernel::default());
    let gear = add(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "plane": "xy", "module": 2, "teeth": 30,
               "face_width": 10, "bore": {"type": "plain", "diameter": 12}, "keyway": true,
               "hub": {"diameter": 30, "length": 8}, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, gear), FeatureStatus::Ok);
    let specs = specs(&doc);
    // The teeth, the hub and the bore with its keyway (4 curves).
    assert_eq!(specs.len(), 3, "{specs:?}");
    assert!(specs[1].contains("from 10 length 8"), "{}", specs[1]);
    assert!(
        specs[2].contains("curves 4 from -1 length 20"),
        "{}",
        specs[2]
    );
    let threaded = add(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "plane": "xy", "center": [100, 0], "module": 2,
               "teeth": 30, "face_width": 10,
               "bore": {"type": "thread", "thread": {"designation": "M8x1.25"}},
               "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, threaded), FeatureStatus::Ok);
}

#[test]
fn gears_mesh_with_gears_and_racks() {
    let mut doc = Document::new(MockKernel::default());
    let pinion = add(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "plane": "xy", "module": 2, "teeth": 20,
               "face_width": 10, "operation": "new_body"}),
    )
    .unwrap();
    // A 30-tooth gear along the y axis: 50 mm away.
    let gear = add(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "module": 2, "teeth": 30, "face_width": 10,
               "mesh_with": {"gear": pinion, "angle": "90 deg"}, "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, gear), FeatureStatus::Ok);
    let (min, max) = bounds(&doc, "F2.b0");
    assert!(near((min[1] + max[1]) / 2.0, 50.0), "{min:?} {max:?}");
    // A rack below the pinion: its tips towards it, its reference line
    // 20 mm from the axis.
    let rack = add(
        &mut doc,
        json!({"type": "gear", "kind": "rack", "module": 2, "teeth": 10, "height": 12,
               "face_width": 10, "mesh_with": {"gear": pinion, "angle": "-90 deg"},
               "operation": "new_body"}),
    )
    .unwrap();
    assert_eq!(status(&doc, rack), FeatureStatus::Ok);
    let (min, max) = bounds(&doc, "F3.b0");
    assert!(near(max[1], -18.0), "{min:?} {max:?}");
    assert!(near(min[1], -30.0), "{min:?} {max:?}");
    // Different modules do not mesh.
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "module": 3, "teeth": 30, "face_width": 10,
               "mesh_with": {"gear": pinion}, "operation": "new_body"}),
    );
    assert!(message.contains("different modules"), "{message}");
    // A meshed gear takes no plane.
    let message = rejected(
        &mut doc,
        json!({"type": "gear", "kind": "spur", "plane": "xy", "module": 2, "teeth": 30,
               "face_width": 10, "mesh_with": {"gear": pinion}, "operation": "new_body"}),
    );
    assert!(message.contains("takes its plane from it"), "{message}");
    // The derived values of the mesh.
    let values: Value = serde_json::from_str(
        &doc.query(&json!({"query": "gear_values", "uid": gear}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert!(
        (values["centre_distance"].as_f64().unwrap() - 50.0).abs() < 1.0e-9,
        "{values}"
    );
    assert!((values["outside_diameter"].as_f64().unwrap() - 64.0).abs() < 1.0e-9);
}
