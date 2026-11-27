// SPDX-License-Identifier: MIT
//! Exploded views follow bodies moved into an existing component (Move
//! Features into a Component with an explicit destination, mitcad#2572)
//! with the mock kernel: a step that moved the bodies moves them from
//! their new path, a direction's geometry on them is read at the
//! destination, a view of the destination takes them as its own bodies, a
//! view whose component no longer holds them says they are gone, and Undo
//! puts every path back.
//!
//! The mock's blocks are 10 x 10 x 5 at their component's origin.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
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

/// A 10 x 10 x 5 block in the active component: its sketch, its extrude
/// and its body.
fn block_in(doc: &mut Document<MockKernel>) -> (FeatureUid, FeatureUid, BodyUid) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (sketch, uid, BodyUid::new(uid, 0))
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

fn step<'a>(views: &'a Value, view: &str, step: usize) -> &'a Value {
    let view = views
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == view)
        .unwrap_or_else(|| panic!("no view {view} in {views}"));
    &view["steps"][step]
}

/// A step's occurrences by name and bodies by uid, as the
/// `explode_views` query gives them.
fn moved(doc: &Document<MockKernel>, view: &str, index: usize) -> (Vec<String>, Vec<Value>) {
    let views = query(doc, json!({"query": "explode_views"}));
    let s = step(&views, view, index);
    let names = s["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap().to_owned())
        .collect();
    let bodies = s["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["body"].clone())
        .collect();
    (names, bodies)
}

fn part<'a>(explode: &'a Value, key: &str, value: &Value) -> &'a Value {
    explode["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| &p[key] == value)
        .unwrap_or_else(|| panic!("no part with {key} {value} in {explode}"))
}

fn warnings(explode: &Value) -> Vec<String> {
    explode["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_owned())
        .collect()
}

/// The ticket's case: a block of the design itself, moved by a step of
/// the design's view, goes into Bin (an existing component with a block of
/// its own, 40 mm along x). The step moves Bin's occurrence instead, with
/// what it places; a direction given by an edge of the moved block is read
/// in Bin. Undo puts the body and the edge's path back.
#[test]
fn an_explode_step_follows_its_body_into_an_existing_component() {
    let mut doc = Document::new(MockKernel::default());
    let (sketch, extrusion, body) = block_in(&mut doc);
    let (_, _, stay) = block_in(&mut doc);
    let (bin_component, bin) = doc
        .create_component(Some("Bin"), Transform::translation([40.0, 0.0, 0.0]), true)
        .unwrap();
    block_in(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let edge = some_edge(&doc, body);
    doc.kernel().edge_curves.borrow_mut().insert(
        edge.clone(),
        CurveGeometry::Line {
            start: [0.0; 3],
            end: [0.0, 0.0, 10.0],
        },
    );
    command(
        &mut doc,
        json!({"cmd": "add_explode_view", "name": "Apart", "steps": [
            {"bodies": [body], "distance": 10, "direction": "z"},
            {"bodies": [stay], "distance": 10,
             "direction": {"geometry": {"body": body, "edge": edge}}}]}),
    );
    let views = query(&doc, json!({"query": "explode_views"}));
    let along = &step(&views, "Apart", 1)["direction"];
    assert!(near(&along["resolved"], [0.0, 0.0, 1.0]), "{along}");

    let result = command(
        &mut doc,
        json!({"cmd": "move_to_component", "features": [sketch, extrusion],
               "destination": {"kind": "existing", "component": bin_component,
                               "occurrence": bin.to_string()}}),
    );
    assert_eq!(result["components"][0]["created"], false, "{result}");
    assert_eq!(doc.body_component(body), Some(bin_component));

    // The step moves the body from its new path: Bin's occurrence.
    assert_eq!(moved(&doc, "Apart", 0), (vec!["Bin:1".to_owned()], vec![]));
    let apart = query(&doc, json!({"query": "explode_view", "name": "Apart"}));
    assert!(warnings(&apart).is_empty(), "{apart}");
    let bin_part = part(&apart, "name", &json!("Bin:1"));
    assert!(near(&bin_part["translation"], [0.0, 0.0, 10.0]), "{apart}");
    // The edge is read where the block is now.
    let views = query(&doc, json!({"query": "explode_views"}));
    let along = &step(&views, "Apart", 1)["direction"];
    assert_eq!(along["occurrence_name"], "Bin:1", "{along}");
    assert!(near(&along["resolved"], [0.0, 0.0, 1.0]), "{along}");
    let stay_part = part(&apart, "body", &json!(stay));
    assert!(near(&stay_part["translation"], [0.0, 0.0, 10.0]), "{apart}");

    // Undo: the body of the design again, in the step as it was.
    doc.undo();
    assert_eq!(doc.body_component(body), Some(ComponentUid::ROOT));
    assert_eq!(moved(&doc, "Apart", 0), (vec![], vec![json!(body)]));
    let views = query(&doc, json!({"query": "explode_views"}));
    let along = &step(&views, "Apart", 1)["direction"];
    assert!(along.get("occurrence_name").is_none(), "{along}");
    let apart = query(&doc, json!({"query": "explode_view", "name": "Apart"}));
    assert!(warnings(&apart).is_empty(), "{apart}");
    let body_part = part(&apart, "body", &json!(body));
    assert!(near(&body_part["translation"], [0.0, 0.0, 10.0]), "{apart}");
}

/// Bin:1 holds Arm:1, which has a block of its own and holds Pin:1. Arm's
/// block goes into Bin: the design's view, which moved Arm:1, moves Bin:1
/// too; Bin's view, which moved Arm:1, moves the block as Bin's own body;
/// Arm's view, which moved the block as its own body, no longer holds it
/// and says so. Undo puts every step back.
#[test]
fn views_of_the_source_and_the_destination_follow_the_moved_body() {
    let mut doc = Document::new(MockKernel::default());
    let (bin_component, bin) = doc
        .create_component(Some("Bin"), Transform::IDENTITY, true)
        .unwrap();
    let (arm_component, arm): (ComponentUid, OccurrenceUid) = doc
        .create_component(Some("Arm"), Transform::IDENTITY, true)
        .unwrap();
    let (sketch, extrusion, body) = block_in(&mut doc);
    doc.create_component(Some("Pin"), Transform::translation([0.0, 0.0, 20.0]), true)
        .unwrap();
    block_in(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    command(
        &mut doc,
        json!({"cmd": "add_explode_view", "name": "Apart", "steps": [
            {"occurrences": ["Bin:1/Arm:1"], "distance": 10, "direction": "z"}]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_explode_view", "name": "InBin", "component": "Bin", "steps": [
            {"occurrences": ["Arm:1"], "distance": 5, "direction": "x"}]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_explode_view", "name": "InArm", "component": "Arm", "steps": [
            {"bodies": [body], "distance": 5, "direction": "y"}]}),
    );

    command(
        &mut doc,
        json!({"cmd": "move_to_component", "component": arm_component,
               "features": [sketch, extrusion],
               "source_occurrence": format!("{bin}/{arm}"),
               "destination": {"kind": "existing", "component": bin_component,
                               "occurrence": bin.to_string()}}),
    );
    assert_eq!(doc.body_component(body), Some(bin_component));

    // The design's view: Bin:1 moves with Arm:1, so the block does.
    assert_eq!(
        moved(&doc, "Apart", 0),
        (vec!["Bin:1/Arm:1".to_owned(), "Bin:1".to_owned()], vec![])
    );
    let apart = query(&doc, json!({"query": "explode_view", "name": "Apart"}));
    assert!(warnings(&apart).is_empty(), "{apart}");
    let bin_part = part(&apart, "name", &json!("Bin:1"));
    assert!(near(&bin_part["translation"], [0.0, 0.0, 10.0]), "{apart}");
    // Bin's view: the block is Bin's own now.
    assert_eq!(
        moved(&doc, "InBin", 0),
        (vec!["Arm:1".to_owned()], vec![json!(body)])
    );
    let in_bin = query(&doc, json!({"query": "explode_view", "name": "InBin"}));
    assert!(warnings(&in_bin).is_empty(), "{in_bin}");
    let body_part = part(&in_bin, "body", &json!(body));
    assert!(near(&body_part["translation"], [5.0, 0.0, 0.0]), "{in_bin}");
    // Arm's view: the block left Arm, in words.
    assert_eq!(moved(&doc, "InArm", 0), (vec![], vec![json!(body)]));
    let in_arm = query(&doc, json!({"query": "explode_view", "name": "InArm"}));
    assert!(
        warnings(&in_arm)
            .iter()
            .any(|w| w.ends_with("a body it moved is gone")),
        "{in_arm}"
    );

    doc.undo();
    assert_eq!(doc.body_component(body), Some(arm_component));
    assert_eq!(
        moved(&doc, "Apart", 0),
        (vec!["Bin:1/Arm:1".to_owned()], vec![])
    );
    assert_eq!(moved(&doc, "InBin", 0), (vec!["Arm:1".to_owned()], vec![]));
    assert_eq!(moved(&doc, "InArm", 0), (vec![], vec![json!(body)]));
    let in_arm = query(&doc, json!({"query": "explode_view", "name": "InArm"}));
    assert!(warnings(&in_arm).is_empty(), "{in_arm}");
}
