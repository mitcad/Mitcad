// SPDX-License-Identifier: MIT
//! Face-bounded regions follow their supporting body without Project.

use std::f64::consts::PI;

use mitcad_model::Document;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName, RegionKey};
use mitcad_model_api::DocumentApi;
use serde_json::{Value, json};

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(
        &doc.command(&value.to_string())
            .unwrap_or_else(|e| panic!("{value}: {e}")),
    )
    .unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "sketch", "uid": uid}))
}

fn boundary(doc: &Document<MockKernel>, face: &str, edge: &EdgeName, radius: f64) {
    doc.kernel().face_boundaries.borrow_mut().insert(
        face.to_owned(),
        vec![(
            edge.clone(),
            Curve3::Conic {
                center: [0.0, 0.0, 10.0],
                normal: [0.0, 0.0, 1.0],
                x_axis: [1.0, 0.0, 0.0],
                major: radius,
                minor: radius,
                start: 0.0,
                end: 2.0 * PI,
                closed: true,
            },
        )],
    );
}

fn circle_face() -> (Document<MockKernel>, String, EdgeName) {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 60}),
    );
    let region: RegionKey = sketch(&doc, "F1")["regions"][0]["key"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
        "profiles": [{"sketch": "F1", "region": region.to_string()}],
        "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    let face = FaceName::end(FeatureUid(2), region.clone());
    let edge = EdgeName::new(
        face.clone(),
        FaceName::side(FeatureUid(2), region.segments().next().unwrap().clone()),
    );
    boundary(&doc, &face.to_string(), &edge, 30.0);
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": face.to_string(), "body": "F2.b0"}}),
    );
    assert_eq!(sketch(&doc, "F3")["entities"].as_array().unwrap().len(), 0);
    (doc, face.to_string(), edge)
}

#[test]
fn a_face_octagon_and_its_offset_add_only_one_implicit_region() {
    let (mut doc, face, edge) = circle_face();
    let polygon = command(
        &mut doc,
        json!({"cmd": "sketch.polygon", "sketch": "F3",
        "center": [0, 0], "vertex": [20, 0], "sides": 8, "inscribed": false}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": polygon["made"], "distance": -2}),
    );
    let before = sketch(&doc, "F3");
    let regions = before["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 3, "{before}");
    let outside = regions.iter().find(|r| r["face_bounded"] == true).unwrap();
    assert_eq!(
        regions.iter().filter(|r| r["face_bounded"] == true).count(),
        1
    );
    let key = outside["key"].as_str().unwrap().to_owned();
    assert!(key.contains(&edge.to_string()));
    let area = outside["area"].as_f64().unwrap();
    let expected = PI * 900.0 - 8.0 * 400.0 * (PI / 8.0).tan();
    assert!((area - expected).abs() < 1e-6);
    let profiles = query(&doc, json!({"query": "profiles", "hashes": true}));
    let profile = profiles
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["sketch"] == "F3" && p["region"] == key)
        .unwrap();
    assert_eq!(profile["face_bounded"], true);
    let hash = profile["hash"].clone();
    // The mock follows the changed face geometry; changing the source
    // parameter invalidates the dependent sketch in the normal pipeline.
    boundary(&doc, &face, &edge, 40.0);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    let after = sketch(&doc, "F3");
    assert_eq!(
        before["entities"].as_array().unwrap().len(),
        after["entities"].as_array().unwrap().len()
    );
    let followed = after["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == key)
        .unwrap();
    assert!((followed["area"].as_f64().unwrap() - area - PI * 700.0).abs() < 1e-6);
    let profiles = query(&doc, json!({"query": "profiles", "hashes": true}));
    let changed = profiles
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["sketch"] == "F3" && p["region"] == key)
        .unwrap();
    assert_ne!(changed["hash"], hash);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
        "profiles": [{"sketch": "F3", "region": key}],
        "extent": {"type": "distance", "distance": 2}, "operation": "join"}}),
    );
    assert!(matches!(
        doc.status(FeatureUid(4)),
        Some(mitcad_model::FeatureStatus::Ok)
    ));
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F3", "plane": "xy"}),
    );
    let off_face = sketch(&doc, "F3");
    assert_eq!(off_face["regions"].as_array().unwrap().len(), 2);
    assert!(
        off_face["regions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["face_bounded"] != true)
    );
    let errors = query(&doc, json!({"query": "timeline"})).to_string();
    assert!(errors.contains("has no profile"), "{errors}");
}

#[test]
fn a_line_across_the_face_creates_two_profiles_without_a_closed_sketch_loop() {
    let (mut doc, _, _) = circle_face();
    command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [-40, 0], "end": [40, 0]}),
    );
    let value = sketch(&doc, "F3");
    let regions = value["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 2, "{value}");
    for region in regions {
        assert_eq!(region["face_bounded"], true);
        assert!((region["area"].as_f64().unwrap() - PI * 450.0).abs() < 1e-6);
    }
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F3", "offset": 2}),
    );
    assert!(sketch(&doc, "F3")["regions"].as_array().unwrap().is_empty());
}

#[test]
fn a_face_boundary_in_a_hole_cannot_rebind_to_the_whole_sketch_circle() {
    let (mut doc, _, _) = circle_face();
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": [0, 0], "diameter": 80}),
    );
    let value = sketch(&doc, "F3");
    let regions = value["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 2, "{value}");
    let ring = regions
        .iter()
        .find(|r| r["loops"].as_array().unwrap().len() == 2)
        .unwrap();
    let key = ring["key"].clone();
    assert!(key.as_str().unwrap().contains("b{E{"));
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F3", "plane": "xy"}),
    );
    assert_eq!(sketch(&doc, "F3")["regions"].as_array().unwrap().len(), 1);
    assert!(
        query(
            &doc,
            json!({"query": "profile", "sketch": "F3", "region": key})
        )
        .is_null()
    );
}

#[test]
fn a_tangent_own_circle_survives_losing_its_face_edge_neighbor() {
    let (mut doc, _, _) = circle_face();
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": [35, 0], "diameter": 10}),
    );
    let value = sketch(&doc, "F3");
    let regions = value["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 2, "{value}");
    let own = regions.iter().find(|r| r["face_bounded"] != true).unwrap();
    let key = own["key"].clone();
    assert!(key.as_str().unwrap().contains("b{E{"), "{key}");
    command(
        &mut doc,
        json!({"cmd": "sketch.redefine_plane", "sketch": "F3", "plane": "xy"}),
    );
    let remaining = sketch(&doc, "F3");
    assert_eq!(remaining["regions"].as_array().unwrap().len(), 1);
    assert!((remaining["regions"][0]["area"].as_f64().unwrap() - 25.0 * PI).abs() < 1e-6);
    let profile = query(
        &doc,
        json!({"query": "profile", "sketch": "F3", "region": key}),
    );
    assert!(!profile.is_null(), "{profile}");
    assert_eq!(profile["region"], remaining["regions"][0]["key"]);
}

/// A ring of two sketch circles about the face's edge, referenced by the
/// key its circles alone give it (a design saved before the face's edges
/// bounded profiles): the extrude takes the whole ring, as saved, not the
/// face-bounded piece most like it (mitcad#1182, an extra lobe on reopen).
#[test]
fn a_whole_sketch_region_across_the_face_edge_keeps_its_reference() {
    let (mut doc, _, edge) = circle_face();
    for diameter in [80, 40] {
        command(
            &mut doc,
            json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": [0, 0], "diameter": diameter}),
        );
    }
    let value = sketch(&doc, "F3");
    let outer = value["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "circle" && (e["radius"].as_f64().unwrap() - 40.0).abs() < 1e-9)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let whole = format!("r{{{outer}}}");
    let keys: Vec<&str> = value["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["key"].as_str().unwrap())
        .collect();
    assert!(!keys.contains(&whole.as_str()), "{keys:?}");
    assert!(
        keys.iter()
            .any(|k| k.starts_with(&format!("r{{{outer},")) && k.contains(&edge.to_string())),
        "{keys:?}"
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
        "profiles": [{"sketch": "F3", "region": whole}],
        "extent": {"type": "distance", "distance": 2}, "operation": "new_body"}}),
    );
    assert!(
        matches!(
            doc.status(FeatureUid(4)),
            Some(mitcad_model::FeatureStatus::Ok)
        ),
        "{}",
        query(&doc, json!({"query": "timeline"}))
    );
    let profile = query(
        &doc,
        json!({"query": "profile", "sketch": "F3", "region": whole}),
    );
    assert_eq!(profile["region"], whole.as_str(), "{profile}");
    assert_eq!(profile["exact"], true, "{profile}");
    assert_eq!(profile["regions"], json!([whole]), "{profile}");
}
