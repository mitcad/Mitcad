// SPDX-License-Identifier: MIT
//! A sketch text as a CAM outline (mitcad#2588, GitHub mitcad/Mitcad#745)
//! on the mock kernel (its `O`: two rectangles, the outline and the
//! counter):
//!
//! - An input of sketch curves that names a sketch text is refused, on
//!   `cam_operation_add` and on `cam_operation_edit`, with what to do
//!   instead (Explode Text); any other name that is no curve's id is
//!   refused with how curves are named. Neither message holds an id, so
//!   the notice cleaner shows it whole, and the design is left as it was.
//! - After Explode Text, the letter's curves are an outline: a contour
//!   along them is generated round the letter.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::notice::{FeatureError, user_text_leaks};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The region of a sketch's first rectangle.
const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

const TEXT_REFUSED: &str = "A CAM operation cannot follow a sketch text: Explode Text turns it into sketch curves to pick instead.";

const NAME_REFUSED: &str = "An input of sketch curves names the sketch's curves by their ids, as the sketch query lists them.";

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

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was taken: {result}"),
        Err(e) => e.to_string(),
    }
}

/// A 40 x 30 block 10 high (F2.b0), an `O` 10 high at (10, 8) on a second
/// sketch (F3), a router with a 6 mm flat end mill and a setup round the
/// block with an outside contour of the block's profile (operation 2).
/// The text's id.
fn lettered() -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
            {"cmd": "sketch.create"},
        ]),
    );
    let made = command(
        &mut doc,
        json!({"cmd": "sketch.add_text", "sketch": "F3", "text": "O", "at": [10, 8],
               "height": 10}),
    );
    let text = made["texts"][0].as_str().unwrap().to_owned();
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": {
            "id": "router",
            "machine": {
                "name": "Bench router",
                "controller": "grbl",
                "travel": {"min": [0, 0, -120], "max": [600, 400, 0]},
                "spindle": {"min_rpm": 8000, "max_rpm": 24000, "spin_up_seconds": 2,
                            "nose_diameter": 50, "nose_length": 80},
                "max_feed": 5000,
                "rapid_feed": 6000,
                "tool_changer": {"kind": "manual"}
            },
            "tools": {"tools": [
                {"number": 1, "name": "6 mm flat end mill", "shape": {"kind": "flat"}, "diameter": 6,
                 "flute_length": 18, "length": 25,
                 "cutting": {"spindle_rpm": 18000, "feed": 1200, "plunge_feed": 300}}
            ]}
        }}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"stock": {"kind": "around_bodies", "top": 1}}}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "kind": {"type": "contour", "side": "outside"}, "tool": 1,
            "inputs": [{"kind": "region", "sketch": "F1", "region": R}]}}),
    );
    (doc, text)
}

fn operations(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "cam_operations", "setup": 1}))["operations"]
        .as_array()
        .unwrap()
        .clone()
}

/// A shown message: whole after the notice cleaner, with no id in it.
fn shown_whole(message: &str) {
    assert_eq!(FeatureError::from_text(message).message, message);
    assert_eq!(user_text_leaks(message), None, "{message}");
}

#[test]
fn a_sketch_text_as_curves_is_refused_with_what_to_do_instead() {
    let (mut doc, text) = lettered();
    let before = operations(&doc);
    let depth = doc.undo_depth();
    let contour = |curves: Value| {
        json!({"kind": {"type": "contour", "side": "on"}, "tool": 1,
               "inputs": [{"kind": "curves", "sketch": "F3", "curves": curves}]})
    };
    // Added, alone or after a curve's id; and the existing contour edited
    // to it.
    for curves in [json!([text]), json!(["c1", text])] {
        let message = refused(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": contour(curves)}),
        );
        assert_eq!(message, TEXT_REFUSED);
    }
    let message = refused(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": 2, "changes": {
            "inputs": [{"kind": "curves", "sketch": "F3", "curves": [text]}]}}),
    );
    assert_eq!(message, TEXT_REFUSED);
    // A name that is neither: how curves are named, without an example id.
    for name in ["edge", "c", "C3", "t", "p3"] {
        let message = refused(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": contour(json!([name]))}),
        );
        assert_eq!(message, NAME_REFUSED, "{name}");
    }
    shown_whole(TEXT_REFUSED);
    shown_whole(NAME_REFUSED);
    // Nothing changed: the operations, the undo history and the text.
    assert_eq!(operations(&doc), before);
    assert_eq!(doc.undo_depth(), depth);
    let sketch = query(&doc, json!({"query": "sketch", "uid": "F3"}));
    assert_eq!(sketch["texts"].as_array().unwrap().len(), 1, "{sketch}");
}

#[test]
fn a_contour_follows_the_curves_explode_text_makes() {
    let (mut doc, text) = lettered();
    let exploded = command(
        &mut doc,
        json!({"cmd": "sketch.explode_text", "sketch": "F3", "text": text}),
    );
    // The O's outline: its first four curves (lines), a closed chain.
    let made: Vec<String> = exploded["made"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(made.len(), 8, "{exploded}");
    let outline = &made[0..4];
    let sketch = query(&doc, json!({"query": "sketch", "uid": "F3"}));
    let entity = |id: &str| -> Value {
        sketch["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("no {id} in {sketch}"))
            .clone()
    };
    // The outline's box from its lines' end points.
    let (mut min, mut max) = ([f64::MAX; 2], [f64::MIN; 2]);
    for curve in outline {
        let line = entity(curve);
        assert_eq!(line["type"], "line", "{line}");
        for end in ["start", "end"] {
            let point = entity(line[end].as_str().unwrap());
            for (k, (low, high)) in min.iter_mut().zip(&mut max).enumerate() {
                let v = point["at"][k].as_f64().unwrap();
                *low = low.min(v);
                *high = high.max(v);
            }
        }
    }
    command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "kind": {"type": "contour", "side": "outside"}, "tool": 1,
            "inputs": [{"kind": "curves", "sketch": "F3", "curves": outline}]}}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let letter = &generated["setups"][0]["operations"][1];
    assert_eq!(letter["state"], json!("up_to_date"), "{letter}");
    assert!(letter["cut_length"].as_f64().unwrap() > 0.0, "{letter}");
    // Round the letter, the tool's radius (3 mm) outside its outline (the
    // work zero is the block's front left corner, so work X and Y are the
    // sketch's).
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let path = paths["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == json!("Contour 2"))
        .unwrap_or_else(|| panic!("no Contour 2 in {paths}"))
        .clone();
    let bounds = &path["bounds"];
    for (k, (low, high)) in min.iter().zip(&max).enumerate() {
        let from = bounds[0][k].as_f64().unwrap();
        let to = bounds[1][k].as_f64().unwrap();
        assert!((from - (low - 3.0)).abs() < 0.01, "{bounds} {min:?}");
        assert!((to - (high + 3.0)).abs() < 0.01, "{bounds} {max:?}");
    }
}
