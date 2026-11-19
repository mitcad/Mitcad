// SPDX-License-Identifier: MIT
//! Panelize (mitcad#2005) with the mock kernel, its boxes measured by
//! their bounds: a cabinet of a 600 x 400 x 300 box with 18 mm panels in
//! three steps (the sides, the back, the top and bottom with Remove
//! Remainder), the panels' volumes, names and material; two faces that
//! meet at an edge refused in one step; Result Components; a thickness
//! parameter changed. The geometry on OCCT runs in the UI test
//! (`tools/ui-scripts/panelize.mitcad-ui`).

use std::collections::BTreeMap;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::ComponentUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// A box F1 of 600 x 400 x 300 from the origin (body F1.b0): bottom (−Z),
/// top (+Z), side0 (−Y), side1 (+X), side2 (+Y), side3 (−X).
fn cabinet_box() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let kernel = doc.kernel();
    kernel.box_faces.set(true);
    kernel.box_booleans.set(true);
    kernel.measures.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 600, "width": 400, "height": 300, "operation": "new_body"}}),
    );
    doc
}

fn face(role: &str) -> String {
    format!("F1:{role}")
}

/// Adds a Panelize step; its uid.
fn panelize(doc: &mut Document<MockKernel>, faces: &[&str], extra: Value) -> String {
    let mut def = json!({"type": "panelize", "body": "F1.b0",
                         "faces": faces.iter().map(|f| face(f)).collect::<Vec<_>>(),
                         "thickness": "t"});
    for (key, value) in extra.as_object().into_iter().flatten() {
        def[key] = value.clone();
    }
    let reply = command(doc, json!({"cmd": "add_feature", "def": def}));
    assert!(reply.get("failed_features").is_none(), "{reply}");
    reply["uid"].as_str().unwrap().to_owned()
}

/// The bodies by name: their volumes.
fn volumes(doc: &Document<MockKernel>) -> BTreeMap<String, f64> {
    doc.bodies()
        .into_iter()
        .map(|b| {
            let volume = doc.kernel().mass_properties(b.shape).unwrap().volume;
            (b.name, volume)
        })
        .collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

/// The three steps of a cabinet: the sides, the back, then the top and
/// bottom with Remove Remainder.
fn cabinet(doc: &mut Document<MockKernel>, extra: Value) -> [String; 3] {
    command(
        doc,
        json!({"cmd": "add_parameter", "name": "t", "expression": "18 mm", "unit": "mm"}),
    );
    let sides = panelize(doc, &["side3", "side1"], extra.clone());
    let back = panelize(doc, &["side2"], extra.clone());
    let mut last = extra;
    last["remove_remainder"] = json!(true);
    let ends = panelize(doc, &["top", "bottom"], last);
    [sides, back, ends]
}

fn expect_cabinet(doc: &Document<MockKernel>, t: f64) {
    let volumes = volumes(doc);
    let side = 400.0 * 300.0 * t;
    let back = (600.0 - 2.0 * t) * 300.0 * t;
    let end = (600.0 - 2.0 * t) * (400.0 - t) * t;
    let expected = [
        ("Left", side),
        ("Right", side),
        ("Back", back),
        ("Top", end),
        ("Bottom", end),
    ];
    assert_eq!(
        volumes.len(),
        5,
        "five panels and no remainder: {volumes:?}"
    );
    for (name, volume) in expected {
        let found = volumes.get(name).copied();
        assert!(
            found.is_some_and(|v| near(v, volume)),
            "{name}: {found:?}, expected {volume}"
        );
    }
}

#[test]
fn a_cabinet_in_three_steps() {
    let mut doc = cabinet_box();
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F1.b0", "material": "aluminum"}),
    );
    cabinet(&mut doc, json!({}));
    expect_cabinet(&doc, 18.0);
    // The volumes the ticket names, to the cubic millimetre.
    let volumes = volumes(&doc);
    assert!(near(volumes["Left"], 2_160_000.0));
    assert!(near(volumes["Back"], 3_045_600.0));
    assert!(near(volumes["Top"], 3_878_064.0));
    // No remainder: the box's own body is gone.
    let bodies = query(&doc, json!({"query": "bodies"}));
    let uids: Vec<&str> = bodies
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["uid"].as_str().unwrap())
        .collect();
    assert!(!uids.contains(&"F1.b0"), "{uids:?}");
    // The panels keep the body's material.
    for body in bodies.as_array().unwrap() {
        assert_eq!(body["material"], json!("aluminum"), "{body}");
    }
}

#[test]
fn each_step_leaves_the_rest_of_the_body() {
    let mut doc = cabinet_box();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "t", "expression": "18 mm", "unit": "mm"}),
    );
    panelize(&mut doc, &["side3", "side1"], json!({}));
    let volumes = volumes(&doc);
    assert_eq!(volumes.len(), 3, "{volumes:?}");
    // The rest of the box, Body1, between the sides.
    assert!(near(volumes["Body1"], 564.0 * 400.0 * 300.0), "{volumes:?}");
}

#[test]
fn faces_that_meet_at_an_edge_are_refused_in_one_step() {
    let mut doc = cabinet_box();
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "t", "expression": "18 mm", "unit": "mm"}),
    );
    let reply = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "panelize", "body": "F1.b0",
               "faces": [face("side1"), face("top")], "thickness": "t"}}),
    );
    let failed = &reply["failed_features"][0];
    let message = failed["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("the panels of two faces overlap"),
        "{reply}"
    );
    // In words: no face names in what a person reads; the faces painted.
    assert!(!message.contains("F1:"), "{message}");
    assert_eq!(
        failed["places"].as_array().map(Vec::len),
        Some(2),
        "{reply}"
    );
    // The box stays as it was.
    let volumes = volumes(&doc);
    assert_eq!(volumes.len(), 1, "{volumes:?}");
}

#[test]
fn result_components_gives_a_component_per_panel() {
    let mut doc = cabinet_box();
    cabinet(&mut doc, json!({"result": "components"}));
    expect_cabinet(&doc, 18.0);
    let components = query(&doc, json!({"query": "components"}));
    let names: Vec<&str> = components["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["uid"] != json!(ComponentUid::ROOT.to_string()))
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        ["Back", "Bottom", "Left", "Right", "Top"],
        "{components}"
    );
    // Each panel is in its own component.
    for body in doc.bodies() {
        assert!(
            doc.body_component(body.uid).is_some_and(|c| !c.is_root()),
            "{} in the root",
            body.name
        );
    }
}

#[test]
fn result_bodies_again_removes_the_components() {
    let mut doc = cabinet_box();
    let [sides, ..] = cabinet(&mut doc, json!({"result": "components"}));
    let reply = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": sides, "def": {"type": "panelize", "body": "F1.b0",
               "faces": [face("side3"), face("side1")], "thickness": "t"}}),
    );
    assert!(reply.get("failed_features").is_none(), "{reply}");
    let components = query(&doc, json!({"query": "components"}));
    let names: Vec<&str> = components["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(
        !names.contains(&"Left") && !names.contains(&"Right"),
        "{names:?}"
    );
    assert!(names.contains(&"Back"), "{names:?}");
    expect_cabinet(&doc, 18.0);
}

#[test]
fn a_thickness_change_updates_the_panels() {
    let mut doc = cabinet_box();
    cabinet(&mut doc, json!({}));
    expect_cabinet(&doc, 18.0);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "t", "expression": "19 mm"}),
    );
    expect_cabinet(&doc, 19.0);
}

#[test]
fn a_zero_thickness_is_refused() {
    let mut doc = cabinet_box();
    let refused = doc
        .command(
            &json!({"cmd": "add_feature", "def": {"type": "panelize", "body": "F1.b0",
                    "faces": [face("top")], "thickness": 0}})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("the panel thickness must be greater than zero"),
        "{refused}"
    );
}
