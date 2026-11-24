// SPDX-License-Identifier: MIT
//! Exploded views that follow the design (mitcad#2328) with the mock
//! kernel: a step's direction given as geometry (an edge of an
//! occurrence's component) follows the edge when its line changes and when
//! the occurrence is turned, `toward` orients it, and a vector step stays
//! as it is; occurrences moved to another parent stay in their steps under
//! their new paths while the view's component holds them, and leave them
//! with a warning in words when they leave it; Measure measures the picks
//! where an explode view shows them.
//!
//! The mock's blocks are 10 x 10 x 5 at their component's origin. The
//! plate (Plate:1) is grounded at the origin; the wall (Wall:1) stands at
//! x = 22, z = 5; the lid (Lid:1) floats at z = 30.

use std::f64::consts::FRAC_PI_2;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::EdgeName;
use mitcad_model::transform::Transform;
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

/// A component with a block placed at `at` in the active component, which
/// stays active.
fn part(doc: &mut Document<MockKernel>, name: &str, at: Transform) -> (OccurrenceUid, BodyUid) {
    let active = doc.assembly().active;
    let (_, occurrence) = doc.create_component(Some(name), at, true).unwrap();
    let body = block_in(doc);
    doc.activate_component(active).unwrap();
    (occurrence, body)
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

/// An edge of a body, as the references name it.
fn some_edge(doc: &Document<MockKernel>, body: BodyUid) -> String {
    let shape = doc.body_shape(body).unwrap();
    let (a, b) = shape.edges.first().expect("the block's edges");
    format!("E{{{a}|{b}}}")
        .parse::<EdgeName>()
        .unwrap()
        .to_string()
}

fn near(a: &Value, b: [f64; 3]) -> bool {
    let a: Vec<f64> = a
        .as_array()
        .unwrap_or_else(|| panic!("not a vector: {a}"))
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    a.len() == 3 && (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
}

fn part_of<'a>(explode: &'a Value, name: &str) -> &'a Value {
    explode["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no part {name} in {explode}"))
}

struct Rig {
    doc: Document<MockKernel>,
    plate: (OccurrenceUid, BodyUid),
    wall: (OccurrenceUid, BodyUid),
    lid: (OccurrenceUid, BodyUid),
}

fn rig() -> Rig {
    let mut doc = Document::new(MockKernel::default());
    let plate = part(&mut doc, "Plate", Transform::IDENTITY);
    let wall = part(&mut doc, "Wall", Transform::translation([22.0, 0.0, 5.0]));
    let lid = part(&mut doc, "Lid", Transform::translation([0.0, 0.0, 30.0]));
    doc.set_occurrence_grounded(plate.0, true).unwrap();
    Rig {
        doc,
        plate,
        wall,
        lid,
    }
}

fn line(end: [f64; 3]) -> CurveGeometry {
    CurveGeometry::Line {
        start: [0.0; 3],
        end,
    }
}

#[test]
fn a_geometry_direction_follows_its_edited_edge() {
    let mut rig = rig();
    let wall_body = rig.wall.1;
    let edge = some_edge(&rig.doc, wall_body);
    // The wall's edge stands up in its component.
    rig.doc
        .kernel()
        .edge_curves
        .borrow_mut()
        .insert(edge.clone(), line([0.0, 0.0, 10.0]));
    let along_edge = json!({"occurrence": "Wall:1",
                            "geometry": {"body": wall_body, "edge": edge}});
    command(
        &mut rig.doc,
        json!({"cmd": "add_explode_view", "name": "Apart", "steps": [
            {"occurrences": ["Lid:1"], "distance": 10, "direction": along_edge},
            {"occurrences": ["Plate:1"], "distance": 5, "direction": [0, 1, 0]}]}),
    );
    let explode =
        |doc: &Document<MockKernel>| query(doc, json!({"query": "explode_view", "name": "Apart"}));
    let shown = explode(&rig.doc);
    assert!(near(
        &part_of(&shown, "Lid:1")["translation"],
        [0.0, 0.0, 10.0]
    ));
    // Kept as the reference, with the direction it gives now.
    let views = query(&rig.doc, json!({"query": "explode_views"}));
    let direction = &views[0]["steps"][0]["direction"];
    assert_eq!(direction["geometry"]["edge"], json!(edge), "{direction}");
    assert_eq!(direction["occurrence_name"], "Wall:1");
    assert!(direction.get("vector").is_none(), "{direction}");
    assert!(near(&direction["resolved"], [0.0, 0.0, 1.0]), "{direction}");

    // The edge changes (an edit of what made it): the step follows it.
    rig.doc
        .kernel()
        .edge_curves
        .borrow_mut()
        .insert(edge.clone(), line([0.0, 10.0, 0.0]));
    let shown = explode(&rig.doc);
    assert!(near(
        &part_of(&shown, "Lid:1")["translation"],
        [0.0, 10.0, 0.0]
    ));
    // The wall turned a quarter about z: the edge, along y in it, is along
    // -x in the design.
    rig.doc
        .set_occurrence_transform(
            rig.wall.0,
            Transform::translation([22.0, 0.0, 5.0])
                .after(&Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], FRAC_PI_2).unwrap()),
            false,
        )
        .unwrap();
    let shown = explode(&rig.doc);
    assert!(near(
        &part_of(&shown, "Lid:1")["translation"],
        [-10.0, 0.0, 0.0]
    ));
    // The step given as a vector stays as it is.
    assert!(near(
        &part_of(&shown, "Plate:1")["translation"],
        [0.0, 5.0, 0.0]
    ));
    let views = query(&rig.doc, json!({"query": "explode_views"}));
    assert!(near(
        &views[0]["steps"][1]["direction"]["vector"],
        [0.0, 1.0, 0.0]
    ));
    assert!(near(
        &views[0]["steps"][0]["direction"]["resolved"],
        [-1.0, 0.0, 0.0]
    ));

    // `toward` (the panel's arrow) orients the geometry: kept as flip.
    let toward = json!({"occurrence": "Wall:1", "toward": [1, 0, 0],
                        "geometry": {"body": wall_body, "edge": edge}});
    command(
        &mut rig.doc,
        json!({"cmd": "add_explode_view", "name": "Out", "steps": [
            {"occurrences": ["Lid:1"], "distance": 10, "direction": toward}]}),
    );
    let views = query(&rig.doc, json!({"query": "explode_views"}));
    let out = &views[1]["steps"][0]["direction"];
    assert_eq!(out["flip"], true, "{out}");
    assert!(out.get("toward").is_none(), "{out}");
    assert!(near(&out["resolved"], [1.0, 0.0, 0.0]), "{out}");
    let shown = query(&rig.doc, json!({"query": "explode_view", "name": "Out"}));
    assert!(near(
        &part_of(&shown, "Lid:1")["translation"],
        [10.0, 0.0, 0.0]
    ));

    // The design file keeps the reference.
    let text = rig.doc.to_json();
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    let kept = query(&copy, json!({"query": "explode_views"}));
    assert_eq!(
        kept[0]["steps"][0]["direction"]["geometry"]["edge"],
        json!(edge)
    );
}

/// Frame:1 (O1) at the origin with a block, holding Bolt:1 (O2) 20 mm up
/// with a block; Pin:1 (O3) in the root at x = 50 with a block.
fn frame() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("Frame"), Transform::IDENTITY, true)
        .unwrap();
    block_in(&mut doc);
    part(&mut doc, "Bolt", Transform::translation([0.0, 0.0, 20.0]));
    doc.activate_component(ComponentUid::ROOT).unwrap();
    part(&mut doc, "Pin", Transform::translation([50.0, 0.0, 0.0]));
    doc
}

fn step_names(doc: &Document<MockKernel>, view: usize, step: usize) -> Vec<String> {
    let views = query(doc, json!({"query": "explode_views"}));
    views[view]["steps"][step]["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap().to_owned())
        .collect()
}

fn warnings(result: &Value) -> Vec<String> {
    result["warnings"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|w| w.as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_reparented_occurrence_stays_in_its_steps_under_its_new_path() {
    let mut doc = frame();
    command(
        &mut doc,
        json!({"cmd": "add_explode_view", "name": "Apart", "steps": [
            {"occurrences": ["Pin:1"], "distance": 10, "direction": "z"},
            {"occurrences": ["Frame:1/Bolt:1"], "distance": 10, "direction": "x"}]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_explode_view", "name": "Inner", "component": "Frame", "steps": [
            {"occurrences": ["Bolt:1"], "distance": 5, "direction": "y"}]}),
    );

    // The pin into Frame:1: still under the root view's component, so its
    // step moves it by its new path; no warning.
    let moved = command(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": "Pin:1", "parent": "Frame:1"}),
    );
    assert!(
        !warnings(&moved).iter().any(|w| w.contains("explode view")),
        "{moved}"
    );
    assert_eq!(step_names(&doc, 0, 0), ["Frame:1/Pin:1"]);
    let apart = query(&doc, json!({"query": "explode_view", "name": "Apart"}));
    assert!(near(
        &part_of(&apart, "Frame:1/Pin:1")["translation"],
        [0.0, 0.0, 10.0]
    ));
    assert!(apart["warnings"].as_array().unwrap().is_empty(), "{apart}");
    // The view of Frame never moved the pin.
    assert_eq!(step_names(&doc, 1, 0), ["Bolt:1"]);

    // The bolt out of Frame into the root: the root's view follows it;
    // Frame's view loses it, in words.
    let moved = command(
        &mut doc,
        json!({"cmd": "reparent_occurrence", "occurrence": "Frame:1/Bolt:1", "parent": ""}),
    );
    assert_eq!(step_names(&doc, 0, 1), ["Bolt:1"]);
    assert!(step_names(&doc, 1, 0).is_empty());
    let said = warnings(&moved);
    assert!(
        said.iter()
            .any(|w| w == "Bolt:1 is no longer in Frame: the explode view Inner no longer moves it"),
        "{moved}"
    );
    assert!(
        !said.iter().any(|w| w.contains("explode view Apart")),
        "{moved}"
    );
    let apart = query(&doc, json!({"query": "explode_view", "name": "Apart"}));
    assert!(near(
        &part_of(&apart, "Bolt:1")["translation"],
        [10.0, 0.0, 0.0]
    ));

    // Undo puts the bolt back into Frame and into both views' steps.
    doc.undo();
    assert_eq!(step_names(&doc, 0, 1), ["Frame:1/Bolt:1"]);
    assert_eq!(step_names(&doc, 1, 0), ["Bolt:1"]);
}

#[test]
fn measure_measures_where_an_explode_view_shows_the_picks() {
    let mut rig = rig();
    rig.doc.kernel().measures_selections.set(true);
    command(
        &mut rig.doc,
        json!({"cmd": "add_explode_view", "name": "Up", "steps": [
            {"occurrences": ["Lid:1"], "distance": 40, "direction": "z"}]}),
    );
    let lid_bottom = json!({"body": rig.lid.1, "face": face(&rig.doc, rig.lid.1, "start"),
                            "occurrence": "Lid:1"});
    let plate_top = json!({"body": rig.plate.1, "face": face(&rig.doc, rig.plate.1, "end"),
                           "occurrence": "Plate:1"});
    let distance = |explode: Option<Value>| {
        let mut request = json!({"query": "measure", "a": lid_bottom, "b": plate_top});
        if let Some(explode) = explode {
            request["explode"] = explode;
        }
        let answer = query(&rig.doc, request);
        (
            answer["between"]["distance"].as_f64().unwrap(),
            answer["between"]["on_a"].clone(),
        )
    };
    // Assembled: the lid's bottom (z 30) 25 mm over the plate's top (z 5).
    let (assembled, _) = distance(None);
    assert!((assembled - 25.0).abs() < 1e-9, "{assembled}");
    // Shown exploded: 40 mm higher, the point on the lid where it is shown.
    let (shown, on_lid) = distance(Some(json!({"name": "Up"})));
    assert!((shown - 65.0).abs() < 1e-9, "{shown}");
    assert!(
        (on_lid[2].as_f64().unwrap() - 70.0).abs() < 1e-9,
        "{on_lid}"
    );
    // Half way into its animation.
    let (half, _) = distance(Some(json!({"name": "Up", "time": 0.5})));
    assert!((half - 45.0).abs() < 1e-9, "{half}");
    // The plate does not move: its top alone is where the design has it.
    let alone = query(
        &rig.doc,
        json!({"query": "measure", "a": plate_top, "explode": {"name": "Up"}}),
    );
    let assembled_alone = query(&rig.doc, json!({"query": "measure", "a": plate_top}));
    assert_eq!(alone["a"], assembled_alone["a"]);
    // A view that is not there is refused in words.
    let error = rig
        .doc
        .query(
            &json!({"query": "measure", "a": plate_top, "explode": {"name": "Gone"}}).to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("there is no explode view Gone"), "{error}");
    // Measuring changed nothing of the design.
    assert_eq!(
        rig.doc.placement(rig.lid.0).unwrap().translation,
        [0.0, 0.0, 30.0]
    );
}
