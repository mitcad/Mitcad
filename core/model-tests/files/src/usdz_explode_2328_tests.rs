// SPDX-License-Identifier: MIT
//! Animated USDZ export of explode views (mitcad#2328) with the mock
//! kernel: a view of a sub-component placed twice animates its occurrence
//! at each placement of the component, and a view of the design animates
//! the root's own body it moves, its mesh in the root. The written files
//! are read back with `mitcad-usdz`'s reader.
//!
//! The mock's blocks are 10 x 10 x 5 at their component's origin. Arm:1
//! (O1) is at the origin with a block and Pin:1 (O2) 5 mm up in it with a
//! block; Arm:2 (O3) is a copy 100 mm along x; the root has a block of its
//! own.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;
use mitcad_usdz::sdf::Value as Usd;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
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

/// The transform of USD's row-vector matrix.
fn from_usd(m: &[[f64; 4]; 4]) -> Transform {
    Transform {
        linear: std::array::from_fn(|r| std::array::from_fn(|c| m[c][r])),
        translation: [m[3][0], m[3][1], m[3][2]],
    }
}

fn layer(path: &str) -> mitcad_usdz::Layer {
    mitcad_usdz::read_package(&std::fs::read(path).unwrap())
        .unwrap()
        .layer
}

/// A prim's `xformOp:transform` samples (time, transform); None when it
/// has none.
fn samples(layer: &mitcad_usdz::Layer, prim: &str) -> Option<Vec<(f64, Transform)>> {
    let path = format!("{prim}.xformOp:transform");
    match layer.field(&path, "timeSamples") {
        None => None,
        Some(Usd::TimeSamples(samples)) => Some(
            samples
                .iter()
                .map(|(t, value)| match value {
                    Usd::Matrix4d(m) => (*t, from_usd(m)),
                    other => panic!("{path}: {other:?}"),
                })
                .collect(),
        ),
        Some(other) => panic!("{path}: {other:?}"),
    }
}

fn temp_path(name: &str) -> String {
    mitcad_model::testing::unique_temp_dir("usdz-explode")
        .join(name)
        .to_str()
        .unwrap()
        .to_owned()
}

fn translations(samples: &[(f64, Transform)]) -> Vec<[f64; 3]> {
    samples.iter().map(|(_, t)| t.translation).collect()
}

fn near(a: &[[f64; 3]], b: &[[f64; 3]]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| (0..3).all(|i| (p[i] - q[i]).abs() < 1e-9))
}

/// Arm:1 and Arm:2 with their pins, and the root's block.
fn arms() -> (Document<MockKernel>, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    let (_, arm) = doc
        .create_component(Some("Arm"), Transform::IDENTITY, true)
        .unwrap();
    block_in(&mut doc);
    doc.create_component(Some("Pin"), Transform::translation([0.0, 0.0, 5.0]), true)
        .unwrap();
    block_in(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let root_body = block_in(&mut doc);
    let copy = doc
        .copy_occurrence(arm, Some(Transform::translation([100.0, 0.0, 0.0])))
        .unwrap();
    assert_eq!(doc.assembly().occurrence_name(copy), "Arm:2");
    (doc, root_body)
}

#[test]
fn a_sub_component_s_explode_view_moves_each_of_its_placements() {
    let (mut doc, _) = arms();
    command(
        &mut doc,
        json!({"cmd": "add_explode_view", "name": "Lift", "component": "Arm", "steps": [
            {"occurrences": ["Pin:1"], "distance": 10, "direction": "z"}]}),
    );
    let depth = doc.undo_depth();
    let path = temp_path("lift.usdz");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path, "animation": {"fps": 2, "explode": "Lift"}}),
    );
    // One second at 2 frames a second, both ends: 0, 0.5 and 1 s.
    assert_eq!(
        result["animation"],
        json!({"fps": 2.0, "start": 0.0, "end": 2.0, "frames": 3,
               "moved": ["Arm:1/Pin:1", "Arm:2/Pin:1"]})
    );
    let layer = layer(&path);
    // The pin rises in its arm, the same at both placements of Arm.
    let rising = [[0.0, 0.0, 5.0], [0.0, 0.0, 10.0], [0.0, 0.0, 15.0]];
    for prim in ["/lift/Arm_1/Pin_1", "/lift/Arm_2/Pin_1"] {
        let pin = samples(&layer, prim).unwrap_or_else(|| panic!("{prim} has no samples"));
        assert_eq!(
            pin.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
            [0.0, 1.0, 2.0]
        );
        assert!(near(&translations(&pin), &rising), "{prim}: {pin:?}");
    }
    // The arms themselves do not move.
    assert!(samples(&layer, "/lift/Arm_1").is_none());
    assert!(samples(&layer, "/lift/Arm_2").is_none());
    // The export changed nothing of the design.
    assert_eq!(doc.undo_depth(), depth);
}

#[test]
fn a_root_body_an_explode_moves_is_animated() {
    let (mut doc, root_body) = arms();
    command(
        &mut doc,
        json!({"cmd": "add_explode_view", "name": "Apart", "steps": [
            {"bodies": [root_body.to_string()], "distance": 20, "direction": "x"},
            {"occurrences": ["Arm:2"], "distance": 10, "direction": "y"}]}),
    );
    let path = temp_path("apart.usdz");
    let result = command(
        &mut doc,
        json!({"cmd": "export", "path": path,
               "animation": {"fps": 1, "explode": "Apart", "frames": 3}}),
    );
    let moved = result["animation"]["moved"].as_array().unwrap();
    assert!(moved.contains(&json!("Arm:2")), "{result}");
    assert!(moved.contains(&json!(doc.body_name(root_body))), "{result}");
    assert_eq!(result["animation"]["frames"], 3);
    let layer = layer(&path);
    // Its two seconds at 0, 1 and 2 s: the block moves along x in the
    // first, then stays; Arm:2 moves along y in the second.
    let root_name = mitcad_usdz::identifier(&doc.body_name(root_body));
    let block = samples(&layer, &format!("/apart/{root_name}")).unwrap();
    assert!(
        near(
            &translations(&block),
            &[[0.0; 3], [20.0, 0.0, 0.0], [20.0, 0.0, 0.0]]
        ),
        "{block:?}"
    );
    let arm2 = samples(&layer, "/apart/Arm_2").unwrap();
    assert!(
        near(
            &translations(&arm2),
            &[[100.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 10.0, 0.0]]
        ),
        "{arm2:?}"
    );
    assert!(samples(&layer, "/apart/Arm_1").is_none());

    // A view that moves nothing written is refused in words.
    command(
        &mut doc,
        json!({"cmd": "add_explode_view", "name": "Still", "steps": [
            {"occurrences": ["Arm:1"], "distance": 0, "direction": "z"}]}),
    );
    let error = doc
        .command(
            &json!({"cmd": "export", "path": temp_path("still.usdz"),
                    "animation": {"fps": 1, "explode": "Still"}})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("the explode view Still moves none of the occurrences or bodies written"),
        "{error}"
    );
}
