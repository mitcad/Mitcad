// SPDX-License-Identifier: MIT
//! A base view of a sheet-metal body's Flat Pattern (mitcad#2280) on the
//! mock kernel: a sheet Face of `w` x 20 (1 mm thick, no bends: the mock
//! kernel develops flat panels only) seen square on along its pattern's
//! frame whatever direction is given, its blank's size following the
//! sketch and the pattern's side, its bend notes turned off and on, no
//! projected view from it, the view kept with the reason in words while
//! the pattern cannot be made, and the design file round trip. The bend
//! lines of real bends (up and down in their two patterns, their notes and
//! a dimension on the view) are checked through OCCT by the CLI script
//! `drawing_flat_pattern_view.json`.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::notice::user_message;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap_or(Value::Null)
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn near(a: &Value, b: f64) -> bool {
    a.as_f64().is_some_and(|a| (a - b).abs() < 1e-6)
}

/// The view of this id as the `drawing` query gives it.
fn view(doc: &Document<MockKernel>, id: u64) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0]["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no view {id}"))
}

/// The blank's size in the view, as the query gives it.
fn size(view: &Value) -> [f64; 2] {
    let size = &view["flat_pattern"]["size"];
    [
        size[0].as_f64().unwrap_or(f64::NAN),
        size[1].as_f64().unwrap_or(f64::NAN),
    ]
}

fn assert_axes(view: &Value, eye: [f64; 3], x: [f64; 3]) {
    let direction = &view["direction"];
    for k in 0..3 {
        assert!(near(&direction["toward_eye"][k], eye[k]), "{direction}");
        assert!(near(&direction["x_axis"][k], x[k]), "{direction}");
    }
}

/// Sheet rules F1, Sketch F2 with a `w` x 20 rectangle at the origin, the
/// sheet Face F3 (F3.b0) on it, and an A3 drawing D1.
fn plate() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "w", "value": 40},
            {"cmd": "add_feature", "def": {"type": "sheet_rules", "thickness": 1, "bend_radius": 2,
              "allowance": {"type": "k_factor", "k": 0.4}, "reference_side": "front",
              "bend_relief": {"type": "round", "width": 1, "depth": 3},
              "corner_relief": {"type": "rectangular", "width": 1, "depth": 3}}},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [0, 0], "width": "w", "height": 20},
            {"cmd": "add_feature", "def": {"type": "sheet_face", "rules": "F1",
              "profiles": [{"sketch": "F2", "region": R}]}},
            {"cmd": "add_drawing", "size": "A3"},
        ]),
    );
    doc
}

#[test]
fn a_flat_pattern_view_shows_the_blank_square_on_and_follows_it() {
    let mut doc = plate();
    // Placed with the Front direction: the blank is seen along its frame
    // all the same (the pattern's own settings, a new item's here).
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "subject": {"type": "flat_pattern", "body": "F3.b0"}, "at": [150, 150],
               "scale": "1:1"}),
    );
    let id = added["view"].as_u64().unwrap();
    let shown = view(&doc, id);
    assert_eq!(
        shown["subject"],
        json!({"type": "flat_pattern", "body": "F3.b0"}),
        "{shown}"
    );
    assert_axes(&shown, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
    let [w, h] = size(&shown);
    assert!(
        (w - 40.0).abs() < 1e-6 && (h - 20.0).abs() < 1e-6,
        "{shown}"
    );
    assert_eq!(shown["flat_pattern"]["body"], "F3.b0", "{shown}");
    assert_eq!(shown["flat_pattern"]["bends"], 0, "{shown}");
    assert_eq!(shown["flat_pattern"]["bend_lines"], json!([]), "{shown}");
    assert_eq!(shown["flat_pattern"]["bend_notes"], true, "{shown}");
    assert!(shown.get("error").is_none(), "{shown}");

    // Its orientation is the pattern's: a direction given is refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id, "direction": "top"}),
    );
    assert!(error.contains("Flat Pattern"), "{error}");
    // No views from its sides.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": id,
               "side": "right"}),
    );
    assert!(error.contains("no projected or section views"), "{error}");
    // Turned on the sheet as any view.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id, "rotation": 90}),
    );
    let turned = view(&doc, id);
    assert!(near(&turned["rotation"], 90.0), "{turned}");
    let [w, h] = size(&turned);
    assert!(
        (w - 20.0).abs() < 1e-6 && (h - 40.0).abs() < 1e-6,
        "{turned}"
    );
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id, "rotation": 0}),
    );

    // The bend notes off and on again; only a Flat Pattern view has them.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id, "bend_notes": false}),
    );
    let quiet = view(&doc, id);
    assert_eq!(quiet["subject"]["bend_notes"], false, "{quiet}");
    assert_eq!(quiet["flat_pattern"]["bend_notes"], false, "{quiet}");
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id, "bend_notes": true}),
    );
    assert!(view(&doc, id)["subject"].get("bend_notes").is_none());
    let folded = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "subject": {"type": "bodies", "bodies": [{"body": "F3.b0"}]}, "at": [300, 150],
               "scale": "1:1"}),
    )["view"]
        .as_u64()
        .unwrap();
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": folded, "bend_notes": false}),
    );
    assert!(error.contains("bend notes"), "{error}");
    assert!(view(&doc, folded).get("flat_pattern").is_none());

    // A wider sketch widens the blank.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "w", "value": 50}),
    );
    let [w, h] = size(&view(&doc, id));
    assert!((w - 50.0).abs() < 1e-6 && (h - 20.0).abs() < 1e-6);

    // Seen from its back (the pattern's own side): mirrored, the eye on
    // the other side.
    command(
        &mut doc,
        json!({"cmd": "set_flat_pattern", "body": "F3.b0", "reference_side": "back"}),
    );
    let back = view(&doc, id);
    assert_axes(&back, [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0]);
    let [w, h] = size(&back);
    assert!((w - 50.0).abs() < 1e-6 && (h - 20.0).abs() < 1e-6, "{back}");

    // A scale leaves the body no sheet metal: the view stays, not drawn,
    // with the reason in words; it comes back with the pattern.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "scale", "bodies": ["F3.b0"],
               "point": {"type": "origin"}, "scale": {"type": "uniform", "factor": 2}}}),
    );
    let broken = view(&doc, id);
    assert_eq!(broken["broken"], true, "{broken}");
    let error = user_message(broken["error"].as_str().unwrap());
    assert!(
        error.contains("Flat Pattern") && error.contains("no longer sheet metal"),
        "{error}"
    );
    assert!(!error.contains("F3"), "in words: {error}");
    command(&mut doc, json!({"cmd": "undo"}));
    let again = view(&doc, id);
    assert!(again.get("broken").is_none(), "{again}");
    assert!((size(&again)[0] - 50.0).abs() < 1e-6, "{again}");

    // The design file keeps the subject; reopened, the view is the same.
    let text = doc.to_json();
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        file["drawings"][0]["sheets"][0]["views"][0]["subject"],
        json!({"type": "flat_pattern", "body": "F3.b0"})
    );
    let mut opened = Document::from_json(&text, MockKernel::default()).unwrap();
    opened.recompute();
    let reopened = view(&opened, id);
    assert_eq!(reopened["subject"], back["subject"]);
    assert_axes(&reopened, [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0]);
    assert_eq!(size(&reopened), size(&back));
}

#[test]
fn a_flat_pattern_view_of_a_body_that_is_no_sheet_is_refused() {
    let mut doc = plate();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [100, 0],
               "length": 10, "width": 10, "height": 10, "operation": "new_body"}}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base",
               "subject": {"type": "flat_pattern", "body": "F4.b0"}, "at": [150, 150]}),
    );
    let words = user_message(&error);
    assert!(words.contains("sheet-metal"), "{error}");
    assert!(!words.contains("F4"), "in words: {error}");
}
