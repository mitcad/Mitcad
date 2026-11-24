// SPDX-License-Identifier: MIT
//! A drawing view of an explode view (mitcad#2113) on the mock kernel: the
//! parts drawn at their explode places, the trail lines as thin phantom
//! lines, a projected view showing its base view's explode, a balloon on
//! an exploded part, and the view drawn assembled again.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn sheet(doc: &Document<MockKernel>) -> Value {
    let answer = doc
        .query(&json!({"query": "drawing", "drawing": "D1"}).to_string())
        .unwrap();
    serde_json::from_str::<Value>(&answer).unwrap()["sheets"][0].clone()
}

/// The lines of the sheet's groups drawn with a pen (of one group, when
/// `id` is given).
fn lines(doc: &Document<MockKernel>, pen: &str, id: Option<&str>) -> Vec<[[f64; 2]; 2]> {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| id.is_none_or(|id| g["id"] == id))
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter(|i| i["pen"] == pen && i.get("line").is_some())
        .map(|i| serde_json::from_value(i["line"].clone()).unwrap())
        .collect()
}

/// How wide the view's visible lines reach along the sheet's x.
fn width(doc: &Document<MockKernel>) -> f64 {
    let xs: Vec<f64> = lines(doc, "thick", Some("view-top"))
        .iter()
        .flat_map(|[a, b]| [a[0], b[0]])
        .collect();
    xs.iter().copied().fold(f64::MIN, f64::max) - xs.iter().copied().fold(f64::MAX, f64::min)
}

/// Base, a 40 x 30 x 10 block (F2.b0), and Pin, a 10 x 10 x 20 block
/// (F4.b0) standing on it at (10, 10, 10); an explode view that moves the
/// pin 60 along x; a drawing with a top view at 1:1.
fn assembly() -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "create_component", "name": "Base"}));
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    let pin = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Pin",
               "transform": {"translation": [10, 10, 10]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "add_explode_view", "steps": [
            {"occurrences": ["Pin:1"], "distance": 60, "direction": "x"}]}),
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    (doc, pin)
}

#[test]
fn a_view_of_an_explode_view_draws_the_parts_apart_with_their_trails() {
    let (mut doc, pin) = assembly();
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "explode": "Nope"}),
    );
    assert!(error.contains("there is no explode view Nope"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [150, 150], "scale": "1:1", "explode": "Explode1"}),
    );
    // The pin 60 to the right of where it stands: the view reaches from
    // the base's left side to the pin's right one, 80 wide; one trail line,
    // from the pin's centre to where the step took it.
    assert!((width(&doc) - 80.0).abs() < 1e-6, "{}", width(&doc));
    let trails = lines(&doc, "phantom", Some("view-top"));
    assert_eq!(trails.len(), 1, "{trails:?}");
    let [a, b] = trails[0];
    assert!(((b[0] - a[0]).abs() - 60.0).abs() < 1e-6 && (b[1] - a[1]).abs() < 1e-6);

    // A projected view shows its base view's explode, trail and all.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1,
               "side": "below"}),
    );
    assert_eq!(lines(&doc, "phantom", None).len(), 2);
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 2, "explode": "Explode1"}),
    );
    assert!(
        error.contains("only a base view shows an explode view"),
        "{error}"
    );

    // A parts list and a balloon on the exploded pin, as on an assembled
    // view.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "parts_list",
               "at": [300, 280]}),
    );
    let pin_body: BodyUid = "F4.b0".parse().unwrap();
    let edge = doc
        .kernel()
        .edges(doc.body_shape(pin_body).unwrap())
        .unwrap()
        .into_iter()
        .find_map(|e| e.name)
        .unwrap();
    let balloon = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "balloon",
               "picks": [{"body": "F4.b0", "occurrence": pin, "edge": edge}]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let attached = sheet(&doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == balloon)
        .cloned()
        .unwrap();
    assert_eq!(attached["broken"], false, "{attached}");

    // The design as assembled again: no trails, the pin over the base.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "explode": ""}),
    );
    assert!(lines(&doc, "phantom", None).is_empty());
    assert!((width(&doc) - 40.0).abs() < 1e-6, "{}", width(&doc));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(lines(&doc, "phantom", None).len(), 2);
    // Its explode view removed: drawn assembled.
    command(
        &mut doc,
        json!({"cmd": "remove_explode_view", "name": "Explode1"}),
    );
    assert!(lines(&doc, "phantom", None).is_empty());
}
