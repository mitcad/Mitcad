// SPDX-License-Identifier: MIT
//! What the application's browser and timeline use (U3): feature
//! visibility, dependents and the reorder check.

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

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

/// Sketch1 (F1) with a rectangle, Extrude1 (F2) and Extrude2 (F3) of its
/// profile, Plane1 (F4) 10 mm above XY.
fn part() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    for distance in [20, 5] {
        command(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "extrude",
                   "profiles": [{"sketch": "F1", "region": REGION}],
                   "extent": {"type": "distance", "distance": distance}, "operation": "new_body"}}),
        );
    }
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 10}}}),
    );
    doc
}

/// What the `timeline` query lists as shown, per feature: `visible`, or
/// null for features without a light bulb.
fn visibility(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.get("visible").cloned().unwrap_or(Value::Null))
        .collect()
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

fn extrude(doc: &mut Document<MockKernel>, sketch: &str, region: &Value) -> String {
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    added["uid"].as_str().unwrap().to_owned()
}

#[test]
fn sketches_and_construction_features_are_shown_and_hidden_with_undo_and_saved() {
    let mut doc = part();
    // The used sketch is hidden, the plane shown; nothing is set.
    assert_eq!(
        visibility(&doc),
        [json!(false), Value::Null, Value::Null, json!(true)]
    );
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    assert_eq!(light_bulb(&doc, "F4"), SHOWN);
    let result = command(
        &mut doc,
        json!({"cmd": "set_feature_visible", "uid": "F1", "visible": true}),
    );
    assert_eq!(result["recomputed"], 0);
    assert_eq!(doc.undo_label(), Some("Show Sketch1"));
    set_visible(&mut doc, "F4", false);
    assert_eq!(doc.undo_label(), Some("Hide Plane1"));
    assert_eq!(
        visibility(&doc),
        [json!(true), Value::Null, Value::Null, json!(false)]
    );
    assert_eq!(light_bulb(&doc, "F1"), SHOWN_BY_HAND);
    let plane = query(&doc, json!({"query": "feature", "uid": "F4"}));
    assert_eq!(
        (plane["visible"].clone(), plane["visible_set"].clone()),
        HIDDEN_BY_HAND
    );
    assert!(
        query(&doc, json!({"query": "feature", "uid": "F2"}))
            .get("visible")
            .is_none()
    );

    // Saved with the feature, and read back.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(file["features"][0]["visible"], true);
    assert_eq!(file["features"][3]["visible"], false);
    assert!(file["features"][1].get("visible").is_none());
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(
        visibility(&loaded),
        [json!(true), Value::Null, Value::Null, json!(false)]
    );
    assert_eq!(loaded.to_json(), saved);

    // The same value is no undo step; undo takes it back.
    set_visible(&mut doc, "F4", false);
    assert_eq!(doc.undo_label(), Some("Hide Plane1"));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        visibility(&doc),
        [json!(true), Value::Null, Value::Null, json!(true)]
    );
    assert_eq!(light_bulb(&doc, "F4"), SHOWN);

    // Back to the default: the light bulb is forgotten, not kept.
    set_visible(&mut doc, "F1", false);
    assert_eq!(doc.undo_label(), Some("Hide Sketch1"));
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    set_visible(&mut doc, "F4", false);
    set_visible(&mut doc, "F4", true);
    assert_eq!(doc.undo_label(), Some("Show Plane1"));
    assert_eq!(light_bulb(&doc, "F4"), SHOWN);
    assert!(!doc.to_json().contains("\"visible\""));

    // Bodies have their own light bulbs.
    let error = doc
        .command(&json!({"cmd": "set_feature_visible", "uid": "F2", "visible": false}).to_string())
        .unwrap_err();
    assert!(
        error.0.contains("Extrude1 has no visibility of its own"),
        "{error}"
    );

    // Deleting a feature forgets its visibility.
    set_visible(&mut doc, "F1", true);
    command(
        &mut doc,
        json!({"cmd": "delete_feature", "uid": "F1", "dependents": true}),
    );
    assert!(!doc.to_json().contains("\"visible\""));
}

/// mitcad#7: a sketch hides when a feature first uses it and shows again
/// when the last one goes; the user's light bulb wins in between.
#[test]
fn a_sketch_hides_with_its_first_consumer_and_shows_after_its_last() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let mut regions = Vec::new();
    for corner in [[0, 0], [100, 0]] {
        let added = command(
            &mut doc,
            json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": corner,
                   "width": 60, "height": 40}),
        );
        regions.push(added["region"].clone());
    }
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);

    // One of two profiles extruded: hidden, in the extrusion's undo step.
    let first = extrude(&mut doc, "F1", &regions[0]);
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    assert_eq!(doc.undo_label(), Some("Add Extrude1"));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);

    // Shown by hand, it stays shown through a recompute and a second
    // consumer.
    set_visible(&mut doc, "F1", true);
    assert_eq!(light_bulb(&doc, "F1"), SHOWN_BY_HAND);
    command(&mut doc, json!({"cmd": "recompute"}));
    assert_eq!(light_bulb(&doc, "F1"), SHOWN_BY_HAND);
    let second = extrude(&mut doc, "F1", &regions[1]);
    assert_eq!(light_bulb(&doc, "F1"), SHOWN_BY_HAND);

    // Deleting one of two consumers changes nothing; deleting the last
    // shows the sketch by default, in the delete's undo step.
    set_visible(&mut doc, "F1", false);
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    command(&mut doc, json!({"cmd": "delete_feature", "uid": second}));
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    set_visible(&mut doc, "F1", true);
    command(&mut doc, json!({"cmd": "delete_feature", "uid": first}));
    assert_eq!(doc.undo_label(), Some("Delete Extrude1"));
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(light_bulb(&doc, "F1"), SHOWN_BY_HAND);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);

    // An unused sketch hidden by hand: its first consumer hides it by
    // default, so deleting that consumer shows it again.
    set_visible(&mut doc, "F1", false);
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN_BY_HAND);
    let third = extrude(&mut doc, "F1", &regions[0]);
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    command(&mut doc, json!({"cmd": "delete_feature", "uid": third}));
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);

    // An edit that moves the consumer to another sketch shows the first
    // and hides the second.
    let fourth = extrude(&mut doc, "F1", &regions[0]);
    let sketch = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"].clone();
    let other = command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": sketch, "center": [0, 100], "diameter": 20}),
    );
    let sketch = sketch.as_str().unwrap();
    assert_eq!(light_bulb(&doc, sketch), SHOWN);
    // Sketch1, the extrusion, the new sketch: the extrusion goes last.
    command(
        &mut doc,
        json!({"cmd": "reorder_feature", "uid": fourth, "index": 2}),
    );
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": fourth, "def": {"type": "extrude",
               "profiles": [{"sketch": sketch, "region": other["region"]}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);
    assert_eq!(light_bulb(&doc, sketch), HIDDEN);
}

/// mitcad#7: suppressed consumers and consumers after the timeline marker
/// still use the sketch.
#[test]
fn suppressing_or_rolling_back_the_consumer_keeps_the_sketch_hidden() {
    let mut doc = part();
    command(&mut doc, json!({"cmd": "suppress_feature", "uid": "F2"}));
    command(&mut doc, json!({"cmd": "suppress_feature", "uid": "F3"}));
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    command(&mut doc, json!({"cmd": "set_marker", "position": 1}));
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F2", "suppressed": false}),
    );
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
}

/// mitcad#7: a design file without light bulbs (as every file saved
/// before the model owned the rule) opens with the default.
#[test]
fn design_files_without_light_bulbs_open_with_the_default() {
    let saved = part().to_json();
    assert!(!saved.contains("\"visible\""));
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    // Before and after the first recompute: the rule needs no results.
    assert_eq!(light_bulb(&loaded, "F1"), HIDDEN);
    loaded.recompute();
    assert_eq!(light_bulb(&loaded, "F1"), HIDDEN);
    assert_eq!(light_bulb(&loaded, "F4"), SHOWN);
}

#[test]
fn dependents_and_the_reorder_check_tell_before_the_command() {
    let mut doc = part();
    assert_eq!(
        query(&doc, json!({"query": "dependents", "uid": "F1"})),
        json!({"dependents": [{"uid": "F2", "name": "Extrude1"}, {"uid": "F3", "name": "Extrude2"}]})
    );
    assert_eq!(
        query(&doc, json!({"query": "dependents", "uid": "F4"})),
        json!({"dependents": []})
    );
    assert!(
        doc.query(&json!({"query": "dependents", "uid": "F9"}).to_string())
            .is_err()
    );

    // The plane can go first; an extrusion cannot come before its sketch.
    assert_eq!(
        query(
            &doc,
            json!({"query": "can_reorder", "uid": "F4", "index": 0})
        ),
        json!({"ok": true})
    );
    let refused = query(
        &doc,
        json!({"query": "can_reorder", "uid": "F2", "index": 0}),
    );
    assert_eq!(refused["ok"], false);
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .starts_with("Extrude1 cannot move there"),
        "{refused}"
    );
    assert_eq!(
        query(
            &doc,
            json!({"query": "can_reorder", "uid": "F2", "index": 9})
        )["ok"],
        false
    );
    // The check changes nothing; the command does what it allowed.
    assert_eq!(doc.undo_label(), Some("Add Plane1"));
    command(
        &mut doc,
        json!({"cmd": "reorder_feature", "uid": "F4", "index": 0}),
    );
    assert_eq!(doc.undo_label(), Some("Move Plane1"));
    let order: Vec<Value> = query(&doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["uid"].clone())
        .collect();
    assert_eq!(order, [json!("F4"), json!("F1"), json!("F2"), json!("F3")]);
    assert!(
        doc.command(&json!({"cmd": "reorder_feature", "uid": "F2", "index": 0}).to_string())
            .is_err()
    );
}

#[test]
fn the_timeline_lists_the_sketches_each_feature_uses() {
    let mut doc = part();
    // Sketch2 (F5) on Plane1 with a square, Loft1 (F6) from Sketch1 to it.
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "F4"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [10, 10], "width": 20, "height": 20}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "loft", "sections": [
                   {"type": "profile", "sketch": "F1", "region": REGION},
                   {"type": "profile", "sketch": "F5", "region": REGION}],
               "operation": "new_body"}}),
    );
    let sketches = |doc: &Document<MockKernel>| -> Vec<(Value, Value)> {
        query(doc, json!({"query": "timeline"}))["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    f["uid"].clone(),
                    f.get("sketches").cloned().unwrap_or(Value::Null),
                )
            })
            .collect()
    };
    // Both extrusions under Sketch1, the loft under both sketches in
    // timeline order; the sketches and the plane use none.
    assert_eq!(
        sketches(&doc),
        [
            (json!("F1"), Value::Null),
            (json!("F2"), json!(["F1"])),
            (json!("F3"), json!(["F1"])),
            (json!("F4"), Value::Null),
            (json!("F5"), Value::Null),
            (json!("F6"), json!(["F1", "F5"])),
        ]
    );
    // Rolled back and suppressed features keep them.
    command(&mut doc, json!({"cmd": "set_marker", "position": 2}));
    command(
        &mut doc,
        json!({"cmd": "suppress_feature", "uid": "F2", "suppressed": true}),
    );
    assert_eq!(sketches(&doc)[1].1, json!(["F1"]));
    assert_eq!(sketches(&doc)[5].1, json!(["F1", "F5"]));
}

/// The id of the curve a sketch command made (its `entities`).
fn made_curve(result: &Value) -> String {
    result["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|e| e.starts_with('c'))
        .unwrap_or_else(|| panic!("no curve in {result}"))
        .to_owned()
}

/// Sketch1 (F1) on XY: a 60 by 40 rectangle round the origin (its region
/// returned) and two lines across the YZ plane, from (-10, 60) to (10, 60)
/// and from (-10, 80) to (10, 80) (their curves returned).
fn source_sketch() -> (Document<MockKernel>, Value, [String; 2]) {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let rectangle = command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-30, -20],
               "width": 60, "height": 40}),
    );
    let lines = [60, 80].map(|y| {
        made_curve(&command(
            &mut doc,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [-10, y],
                   "end": [10, y]}),
        ))
    });
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);
    (doc, rectangle["region"].clone(), lines)
}

/// What the `timeline` query lists as the sketches `uid` uses.
fn sketches_of(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == uid)
        .and_then(|f| f.get("sketches").cloned())
        .unwrap_or(Value::Null)
}

/// mitcad#2045: `make` creates a sketch or a 3D sketch and returns its
/// uid; `pick` projects or intersects one of Sketch1's curves into it.
/// Sketch1 stays shown through both picks (the second curve is picked as
/// the user picks it in the same Project), and an extrusion of its profile
/// afterwards still hides it, deleting the extrusion shows it again. A
/// light bulb set off by hand stays off through the picks. With `linked`
/// the timeline still lists Sketch1 under the sketch that projects it.
fn the_source_sketch_stays_as_it_is(
    linked: bool,
    make: impl Fn(&mut Document<MockKernel>) -> String,
    pick: impl Fn(&mut Document<MockKernel>, &str, &str),
) {
    let (mut doc, region, lines) = source_sketch();
    let target = make(&mut doc);
    for line in &lines {
        pick(&mut doc, target.as_str(), line.as_str());
        assert_eq!(light_bulb(&doc, "F1"), SHOWN, "after {line}");
    }
    if linked {
        assert_eq!(sketches_of(&doc, &target), json!(["F1"]));
    }
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);

    // A consumer of its profile hides it as before.
    let extrusion = extrude(&mut doc, "F1", &region);
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    command(&mut doc, json!({"cmd": "delete_feature", "uid": extrusion}));
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);

    // Hidden by hand before the picks: it stays hidden.
    let (mut doc, region, lines) = source_sketch();
    set_visible(&mut doc, "F1", false);
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN_BY_HAND);
    let target = make(&mut doc);
    for line in &lines {
        pick(&mut doc, target.as_str(), line.as_str());
        assert_eq!(light_bulb(&doc, "F1"), HIDDEN_BY_HAND, "after {line}");
    }
    // The extrusion is its first consumer: the default hides it, and
    // deleting the extrusion shows it.
    let extrusion = extrude(&mut doc, "F1", &region);
    assert_eq!(light_bulb(&doc, "F1"), HIDDEN);
    command(&mut doc, json!({"cmd": "delete_feature", "uid": extrusion}));
    assert_eq!(light_bulb(&doc, "F1"), SHOWN);
}

fn new_sketch(doc: &mut Document<MockKernel>, plane: &str) -> String {
    command(doc, json!({"cmd": "sketch.create", "plane": plane}))["uid"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// mitcad#2045: projecting another sketch's curves, linked or not, does
/// not use that sketch.
#[test]
fn projecting_a_sketchs_curves_leaves_it_shown() {
    for linked in [true, false] {
        the_source_sketch_stays_as_it_is(
            linked,
            |doc| new_sketch(doc, "xy"),
            |doc, target, curve| {
                command(
                    doc,
                    json!({"cmd": "sketch.project", "sketch": target,
                           "source": {"sketch": "F1", "curve": curve}, "linked": linked}),
                );
            },
        );
    }
}

/// mitcad#2045: Intersect with another sketch's curves (a section) does
/// not use that sketch.
#[test]
fn intersecting_a_sketchs_curves_leaves_it_shown() {
    the_source_sketch_stays_as_it_is(
        true,
        |doc| new_sketch(doc, "yz"),
        |doc, target, curve| {
            command(
                doc,
                json!({"cmd": "sketch.project", "sketch": target,
                       "source": {"sketch": "F1", "curve": curve}, "linked": true,
                       "section": true}),
            );
        },
    );
}

/// mitcad#2045: a 3D sketch's linked projection of another sketch's curve
/// does not use that sketch.
#[test]
fn a_3d_sketchs_projection_of_a_sketchs_curves_leaves_it_shown() {
    the_source_sketch_stays_as_it_is(
        true,
        |doc| {
            command(doc, json!({"cmd": "sketch3d.create"}))["uid"]
                .as_str()
                .unwrap()
                .to_owned()
        },
        |doc, target, curve| {
            command(
                doc,
                json!({"cmd": "sketch3d.include", "sketch": target,
                       "source": {"sketch": "F1", "curve": curve}, "projected": true}),
            );
        },
    );
}
