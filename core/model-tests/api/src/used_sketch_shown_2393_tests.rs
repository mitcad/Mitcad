// SPDX-License-Identifier: MIT
//! Preferences › Sketch › Hide a sketch when a feature uses it
//! (mitcad#2393, GitHub mitcad/Mitcad#652): with it off
//! ([`Document::set_hide_used_sketches`]) a shown planar sketch that a
//! feature starts to use keeps its light bulb on, set in the feature's own
//! undo step; a hidden one stays hidden, sketches already hidden are not
//! touched, and on again the default of mitcad#7 applies.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// A feature's `visible` and `visible_set` in the `timeline` query.
fn light_bulb(doc: &Document<MockKernel>, uid: &str) -> (Value, Value) {
    let timeline = query(doc, json!({"query": "timeline"}));
    let feature = timeline["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == uid)
        .unwrap_or_else(|| panic!("{uid} is not in {timeline}"))
        .clone();
    (feature["visible"].clone(), feature["visible_set"].clone())
}

const SHOWN: (Value, Value) = (Value::Bool(true), Value::Bool(false));
const HIDDEN: (Value, Value) = (Value::Bool(false), Value::Bool(false));
const SHOWN_BY_HAND: (Value, Value) = (Value::Bool(true), Value::Bool(true));
const HIDDEN_BY_HAND: (Value, Value) = (Value::Bool(false), Value::Bool(true));

fn set_visible(doc: &mut Document<MockKernel>, uid: &str, visible: bool) {
    command(
        doc,
        json!({"cmd": "set_feature_visible", "uid": uid, "visible": visible}),
    );
}

/// A new sketch on XY with a 60 x 40 rectangle at `corner`: its uid and
/// the rectangle's region.
fn sketch_with_rectangle(doc: &mut Document<MockKernel>, corner: [i32; 2]) -> (String, Value) {
    let sketch = command(doc, json!({"cmd": "sketch.create"}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    let added = command(
        doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": corner,
               "width": 60, "height": 40}),
    );
    (sketch, added["region"].clone())
}

fn extrude(doc: &mut Document<MockKernel>, sketch: &str, region: &Value) -> String {
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    added["uid"].as_str().unwrap().to_owned()
}

/// Off: the extruded sketch stays shown, in the extrusion's undo step;
/// undo and redo take the light bulb with it; it is saved as a light bulb
/// set by hand. On again: the next sketch hides when extruded.
#[test]
fn a_shown_sketch_stays_shown_when_extruded_with_the_preference_off() {
    let mut doc = Document::new(MockKernel::default());
    assert!(doc.hide_used_sketches());
    doc.set_hide_used_sketches(false);
    assert!(!doc.hide_used_sketches());
    let (first, region) = sketch_with_rectangle(&mut doc, [0, 0]);
    assert_eq!(light_bulb(&doc, &first), SHOWN);

    let extrusion = extrude(&mut doc, &first, &region);
    assert_eq!(light_bulb(&doc, &first), SHOWN_BY_HAND);
    assert_eq!(doc.undo_label(), Some("Add Extrude1"));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(light_bulb(&doc, &first), SHOWN);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(light_bulb(&doc, &first), SHOWN_BY_HAND);

    // Saved and read back as a light bulb set by hand, whatever the
    // preference of the document that reads it.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["features"][0]["visible"], true);
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(light_bulb(&loaded, &first), SHOWN_BY_HAND);

    // The eye still hides it; then it follows the default, hidden.
    set_visible(&mut doc, &first, false);
    assert_eq!(light_bulb(&doc, &first), HIDDEN);
    // Deleting the last consumer shows it again, as with the preference on.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": extrusion}));
    assert_eq!(light_bulb(&doc, &first), SHOWN);

    // On again: a second sketch hides when extruded.
    doc.set_hide_used_sketches(true);
    let (second, region) = sketch_with_rectangle(&mut doc, [100, 0]);
    assert_eq!(light_bulb(&doc, &second), SHOWN);
    extrude(&mut doc, &second, &region);
    assert_eq!(light_bulb(&doc, &second), HIDDEN);
    assert_eq!(light_bulb(&doc, &first), SHOWN);
}

/// Off: a sketch hidden by hand while unused stays hidden when a feature
/// starts to use it (its light bulb goes back to the default, which hides
/// it); a sketch that a feature uses already, hidden as the preference
/// was on, is not touched by turning it off nor by a second consumer.
#[test]
fn hidden_sketches_stay_hidden_with_the_preference_off() {
    let mut doc = Document::new(MockKernel::default());
    let (used, used_region) = sketch_with_rectangle(&mut doc, [0, 0]);
    extrude(&mut doc, &used, &used_region);
    assert_eq!(light_bulb(&doc, &used), HIDDEN);

    doc.set_hide_used_sketches(false);
    assert_eq!(light_bulb(&doc, &used), HIDDEN);
    extrude(&mut doc, &used, &used_region);
    assert_eq!(light_bulb(&doc, &used), HIDDEN);

    let (hidden, region) = sketch_with_rectangle(&mut doc, [100, 0]);
    set_visible(&mut doc, &hidden, false);
    assert_eq!(light_bulb(&doc, &hidden), HIDDEN_BY_HAND);
    extrude(&mut doc, &hidden, &region);
    assert_eq!(light_bulb(&doc, &hidden), HIDDEN);
}

/// Off: an edit that moves an extrusion from one sketch to a shown one
/// keeps the new one shown and shows the one it left, by default.
#[test]
fn an_edit_that_takes_another_sketch_keeps_it_shown_with_the_preference_off() {
    let mut doc = Document::new(MockKernel::default());
    let (first, first_region) = sketch_with_rectangle(&mut doc, [0, 0]);
    let (second, second_region) = sketch_with_rectangle(&mut doc, [100, 0]);
    let extrusion = extrude(&mut doc, &first, &first_region);
    assert_eq!(light_bulb(&doc, &first), HIDDEN);

    doc.set_hide_used_sketches(false);
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": extrusion, "def": {"type": "extrude",
               "profiles": [{"sketch": second, "region": second_region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    assert_eq!(light_bulb(&doc, &first), SHOWN);
    assert_eq!(light_bulb(&doc, &second), SHOWN_BY_HAND);
}
