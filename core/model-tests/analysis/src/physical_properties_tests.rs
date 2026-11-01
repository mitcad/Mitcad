// SPDX-License-Identifier: MIT
//! Physical Properties' extent (mitcad#1095), on the mock kernel
//! (`MockKernel::physical`: the properties of the bodies' boxes; the
//! counts of its faces and face-pair edges): each body's box along the
//! design's axes, placed by its occurrences, and its face, edge and vertex
//! counts, with the box around all and the summed counts in the total; a
//! selected component counts its own bodies and its sub-components'. A
//! body counts once per occurrence, placed and with the material it has
//! there, and the total is the `center_of_mass` query's (mitcad#1110).

use std::f64::consts::FRAC_PI_2;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::{AnalysisApi, DocumentApi};

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn properties(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn refused(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(answer) => panic!("{query} was answered: {answer}"),
        Err(e) => e.to_string(),
    }
}

/// A 20 x 20 rectangle on XY extruded by `height`, in the active
/// component (sketch `sketch`, the extrusion the next feature).
fn block(sketch: &str, height: f64) -> Value {
    json!([
        {"cmd": "sketch.create"},
        {"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": [0, 0], "width": 20, "height": 20},
        {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": sketch, "region": R}],
            "extent": {"type": "distance", "distance": height}, "operation": "new_body"}},
    ])
}

/// Within 1e-6 mm of each other.
fn near_point(actual: &Value, expected: [f64; 3]) {
    for (i, expected) in expected.into_iter().enumerate() {
        let value = actual[i]
            .as_f64()
            .unwrap_or_else(|| panic!("{actual} has no number {i}"));
        assert!(
            (value - expected).abs() <= 1e-6,
            "{actual} instead of {expected:?}"
        );
    }
}

/// A `bbox` from `min` to `max`, its size the difference.
fn assert_box(bbox: &Value, min: [f64; 3], max: [f64; 3]) {
    near_point(&bbox["min"], min);
    near_point(&bbox["max"], max);
    near_point(&bbox["size"], std::array::from_fn(|i| max[i] - min[i]));
}

fn assert_counts(value: &Value, faces: u64, edges: u64, vertices: u64) {
    let found = json!([value["faces"], value["edges"], value["vertices"]]);
    assert_eq!(found, json!([faces, edges, vertices]), "{value}");
}

fn uids(answer: &Value) -> Vec<String> {
    answer["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["uid"].as_str().unwrap().to_owned())
        .collect()
}

/// Body1 (F2.b0), a 20 x 20 x 10 block at the origin of the root, and
/// Body2 (F4.b0), a 20 mm cube in Arm (C1), placed by O1 at (100, 20, 30)
/// turned 90 degrees about Z: in the design x 80 to 100, y 20 to 40,
/// z 30 to 50.
fn two_boxes() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().physical.set(true);
    command(&mut doc, block("F1", 10.0));
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Arm", "transform": {
            "translation": [100, 20, 30], "rotation": {"axis": [0, 0, 1], "angle": FRAC_PI_2}}}),
    );
    command(&mut doc, block("F3", 20.0));
    doc
}

#[test]
fn boxes_lie_along_the_design_axes_and_counts_add_up() {
    let mut doc = two_boxes();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let answer = properties(&doc, json!({"query": "properties"}));
    assert_eq!(uids(&answer), ["F2.b0", "F4.b0"]);
    let body1 = &answer["bodies"][0];
    let body2 = &answer["bodies"][1];
    assert_box(&body1["bbox"], [0.0, 0.0, 0.0], [20.0, 20.0, 10.0]);
    // The cube placed by its occurrence, not in Arm's own coordinates.
    assert_box(&body2["bbox"], [80.0, 20.0, 30.0], [100.0, 40.0, 50.0]);
    assert_counts(body1, 6, 12, 8);
    assert_counts(body2, 6, 12, 8);
    let total = &answer["total"];
    assert_box(&total["bbox"], [0.0, 0.0, 0.0], [100.0, 40.0, 50.0]);
    assert_counts(total, 12, 24, 16);
    // The total area: 1600 and 2400 mm².
    assert!(
        (total["area"].as_f64().unwrap() - 4000.0).abs() <= 1e-6,
        "{total}"
    );
    assert_eq!(answer.get("components"), None);

    // The text answer has them too.
    let text = doc
        .analysis(&json!({"query": "properties"}).to_string(), false)
        .unwrap();
    assert!(
        text.contains(
            "    bounding box [80.000, 20.000, 30.000] to [100.000, 40.000, 50.000], \
             size 20.000 x 20.000 x 20.000 mm\n    6 faces, 12 edges, 8 vertices\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("Total 12 faces, 24 edges, 16 vertices\nTotal: mass"),
        "{text}"
    );
}

#[test]
fn a_selected_component_counts_its_own_bodies_and_its_sub_components() {
    let mut doc = two_boxes();
    // Pin (C2) in Arm, placed by O2 at (5, 7, 11) in Arm, with Body3
    // (F6.b0), 20 x 20 x 5: in the design x 73 to 93, y 25 to 45, z 41
    // to 46.
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Pin", "transform": {"translation": [5, 7, 11]}}),
    );
    command(&mut doc, block("F5", 5.0));
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );

    // Arm by its uid, its name or its occurrence: its own cube and Pin's
    // block, not the root's Body1.
    for arm in [
        json!("C1"),
        json!("Arm"),
        json!({"occurrence": "O1"}),
        json!({"component": "Arm", "occurrence": "Arm:1"}),
    ] {
        let answer = properties(&doc, json!({"query": "properties", "components": [arm]}));
        assert_eq!(uids(&answer), ["F4.b0", "F6.b0"], "{arm}");
        assert_box(
            &answer["bodies"][1]["bbox"],
            [73.0, 25.0, 41.0],
            [93.0, 45.0, 46.0],
        );
        assert_box(
            &answer["total"]["bbox"],
            [73.0, 20.0, 30.0],
            [100.0, 45.0, 50.0],
        );
        assert_counts(&answer["total"], 12, 24, 16);
        assert_eq!(answer["components"][0]["uid"], "C1", "{arm}");
        assert_eq!(answer["components"][0]["name"], "Arm", "{arm}");
    }
    let by_occurrence = properties(
        &doc,
        json!({"query": "properties", "components": [{"occurrence": "O1"}]}),
    );
    assert_eq!(by_occurrence["components"][0]["occurrence"], "Arm:1");

    // Pin alone, as the occurrence path places it; with Body1 asked for
    // besides, Body1 first.
    let pin = properties(
        &doc,
        json!({"query": "properties", "components": [{"occurrence": "O1/O2"}]}),
    );
    assert_eq!(uids(&pin), ["F6.b0"]);
    assert_eq!(pin["components"][0]["name"], "Pin");
    assert_box(
        &pin["total"]["bbox"],
        [73.0, 25.0, 41.0],
        [93.0, 45.0, 46.0],
    );
    assert_counts(&pin["total"], 6, 12, 8);
    let both = properties(
        &doc,
        json!({"query": "properties", "bodies": ["F2.b0"], "components": ["Pin"]}),
    );
    assert_eq!(uids(&both), ["F2.b0", "F6.b0"]);
    assert_box(&both["total"]["bbox"], [0.0, 0.0, 0.0], [93.0, 45.0, 46.0]);

    // The root component is the whole design.
    let root = properties(&doc, json!({"query": "properties", "components": ["C0"]}));
    assert_eq!(uids(&root), ["F2.b0", "F4.b0", "F6.b0"]);
    assert_counts(&root["total"], 18, 36, 24);

    assert_eq!(
        refused(
            &doc,
            json!({"query": "properties", "components": ["Wheel"]})
        ),
        "there is no component “Wheel”"
    );
    assert_eq!(
        refused(
            &doc,
            json!({"query": "properties", "components": [{"component": "Pin", "occurrence": "O1"}]})
        ),
        "occurrence O1 does not place Pin"
    );
    assert_eq!(
        refused(
            &doc,
            json!({"query": "properties", "components": [{"occurrence": "O9"}]})
        ),
        "there is no occurrence “O9”; the components query lists them"
    );
}

/// Within `tolerance` of `expected`.
fn near(actual: &Value, expected: f64, tolerance: f64) {
    let value = actual
        .as_f64()
        .unwrap_or_else(|| panic!("{actual} is no number"));
    assert!(
        (value - expected).abs() <= tolerance,
        "{actual} instead of {expected}"
    );
}

fn vec3(value: &Value) -> [f64; 3] {
    std::array::from_fn(|i| value[i].as_f64().unwrap_or(f64::NAN))
}

/// The `properties` query's total mass and center are the
/// `center_of_mass` query's (mitcad#1110), over as many bodies.
fn assert_total_is_the_center_of_mass(doc: &Document<MockKernel>, answer: &Value) {
    let marker = properties(doc, json!({"query": "center_of_mass"}));
    let total = &answer["total"];
    assert_eq!(
        marker["bodies"].as_u64(),
        Some(answer["bodies"].as_array().unwrap().len() as u64),
        "{marker}"
    );
    near(&marker["mass"], total["mass"].as_f64().unwrap(), 1e-12);
    near(&marker["volume"], total["volume"].as_f64().unwrap(), 1e-6);
    near_point(&marker["center_of_mass"], vec3(&total["center_of_mass"]));
}

/// Steel's density in kg/mm³ (the mock kernel's mass: the box's volume
/// times the density).
const STEEL: f64 = 7.85e-6;

/// A component placed twice and moved (mitcad#1110): its body counts once
/// per occurrence, each entry where the occurrence places it, and the
/// total's mass and center are the `center_of_mass` query's.
#[test]
fn a_component_placed_twice_counts_once_per_occurrence() {
    let mut doc = two_boxes();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    // Arm placed again, then moved: its cube at x 0 to 20, y -50 to -30,
    // z 70 to 90.
    let copy = command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "Arm:1",
               "transform": {"translation": [0, -50, 0]}}),
    );
    let name = copy["name"].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": name.as_str(),
               "transform": {"translation": [0, -50, 70]}}),
    );

    let answer = properties(&doc, json!({"query": "properties"}));
    // The cube once per occurrence, each where it is placed.
    assert_eq!(uids(&answer), ["F2.b0", "F4.b0", "F4.b0"]);
    let bodies = answer["bodies"].as_array().unwrap();
    assert_eq!(bodies[0].get("occurrence"), None, "{answer}");
    assert_eq!(bodies[1]["occurrence"], "Arm:1");
    assert_eq!(bodies[2]["occurrence"], name.as_str());
    near_point(&bodies[1]["center_of_mass"], [90.0, 30.0, 40.0]);
    near_point(&bodies[2]["center_of_mass"], [10.0, -40.0, 80.0]);
    assert_box(&bodies[1]["bbox"], [80.0, 20.0, 30.0], [100.0, 40.0, 50.0]);
    assert_box(&bodies[2]["bbox"], [0.0, -50.0, 70.0], [20.0, -30.0, 90.0]);
    for cube in &bodies[1..] {
        near(&cube["mass"], 8000.0 * STEEL, 1e-12);
        assert_eq!(cube["material"], "steel");
        assert_counts(cube, 6, 12, 8);
    }

    // The total: Body1 (4000 mm³ at (10, 10, 5)) and the cube twice
    // (8000 mm³ each).
    let total = &answer["total"];
    near(&total["mass"], 20000.0 * STEEL, 1e-12);
    near(&total["volume"], 20000.0, 1e-6);
    near_point(&total["center_of_mass"], [42.0, -2.0, 49.0]);
    assert_box(&total["bbox"], [0.0, -50.0, 0.0], [100.0, 40.0, 90.0]);
    assert_counts(total, 18, 36, 24);
    assert_total_is_the_center_of_mass(&doc, &answer);

    // Arm by name counts both its placements; its cube asked for besides
    // still counts once per occurrence.
    for query in [
        json!({"query": "properties", "components": ["Arm"]}),
        json!({"query": "properties", "bodies": ["F4.b0"], "components": ["Arm"]}),
    ] {
        let arm = properties(&doc, query.clone());
        assert_eq!(uids(&arm), ["F4.b0", "F4.b0"], "{query}");
        near(&arm["total"]["mass"], 16000.0 * STEEL, 1e-12);
        near_point(&arm["total"]["center_of_mass"], [50.0, -5.0, 60.0]);
    }
    // One occurrence alone.
    let one = properties(
        &doc,
        json!({"query": "properties", "components": [{"occurrence": name.as_str()}]}),
    );
    assert_eq!(uids(&one), ["F4.b0"]);
    near_point(&one["total"]["center_of_mass"], [10.0, -40.0, 80.0]);

    // The text answer names the occurrence.
    let text = doc
        .analysis(&json!({"query": "properties"}).to_string(), false)
        .unwrap();
    assert!(
        text.contains(&format!("(F4.b0) in {name}: Steel, density 7.85 g/cm3\n")),
        "{text}"
    );
    assert!(
        text.contains(
            "Total: mass 0.157000 kg, volume 20000.000 mm3, \
             center of mass [42.000, -2.000, 49.000]"
        ),
        "{text}"
    );
}

/// Each occurrence with the material it has on its path (mitcad#1110):
/// Arm placed in the root and in Frame, whose default is aluminum.
#[test]
fn each_occurrence_counts_with_the_material_it_has_there() {
    let mut doc = two_boxes();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    // Frame, 100 mm down, with Arm placed in it too: that cube at z -100
    // to -80 takes Frame's default; Arm:1's stays steel.
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Frame",
               "transform": {"translation": [0, 0, -100]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Frame", "material": "aluminum"}),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "Frame"}),
    );
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "Arm:1",
               "transform": {"translation": [0, 0, 0]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );

    let answer = properties(&doc, json!({"query": "properties"}));
    assert_eq!(uids(&answer), ["F2.b0", "F4.b0", "F4.b0"]);
    let bodies = answer["bodies"].as_array().unwrap();
    assert_eq!(bodies[1]["occurrence"], "Arm:1");
    assert_eq!(bodies[1]["material"], "steel");
    assert!(
        bodies[2]["occurrence"]
            .as_str()
            .is_some_and(|path| path.starts_with("Frame:1/Arm:")),
        "{answer}"
    );
    assert_eq!(bodies[2]["material"], "aluminum");
    near(&bodies[2]["density"], 2.70, 1e-12);
    near(&bodies[2]["mass"], 8000.0 * 2.70e-6, 1e-12);
    near_point(&bodies[2]["center_of_mass"], [10.0, 10.0, -90.0]);
    near(
        &answer["total"]["mass"],
        12000.0 * STEEL + 8000.0 * 2.70e-6,
        1e-12,
    );
    assert_total_is_the_center_of_mass(&doc, &answer);
}
