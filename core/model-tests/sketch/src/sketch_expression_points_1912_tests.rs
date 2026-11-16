// SPDX-License-Identifier: MIT
//! Point coordinates written as expressions of parameters in 3D sketches
//! and in the planar sketch's position-only fields (mitcad#1912, after
//! mitcad#1897): a 3D sketch's new point is held by signed Offset
//! dimensions from its origin point and follows the parameters; a place
//! where the command makes no point, a move and a scale refuse them in
//! words; a circular pattern's new centre follows them. Refusals leave the
//! sketch as it was.

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

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

fn set(doc: &mut Document<MockKernel>, name: &str, value: f64) {
    command(
        doc,
        json!({"cmd": "set_parameter", "name": name, "value": value}),
    );
}

fn id(value: &Value) -> String {
    value
        .as_str()
        .unwrap_or_else(|| panic!("{value} is no id"))
        .to_owned()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

/// A document with the parameters w = 60 and h = 40.
fn with_parameters() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "w", "value": 60}),
    );
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "h", "value": 40}),
    );
    d
}

/// The `sketch3d` query of F1.
fn sketch3d(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "sketch3d", "uid": "F1"}))
}

/// Where a 3D sketch's point is (local, solved).
fn at3(s: &Value, point: &str) -> [f64; 3] {
    let p = s["points"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == point)
        .unwrap_or_else(|| panic!("no {point} in {s}"));
    [0, 1, 2].map(|k| p["at"][k].as_f64().unwrap())
}

fn dimension3<'a>(s: &'a Value, k: &str) -> &'a Value {
    s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == k)
        .unwrap_or_else(|| panic!("no {k} in {s}"))
}

/// Whether `point` is attached to the sketch's origin.
fn on_origin(s: &Value, point: &str) -> bool {
    s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["type"] == "attach" && c["point"] == point && c["source"] == "origin")
}

/// What a refused 3D edit must leave as it was.
fn items3(s: &Value) -> Value {
    json!([s["points"], s["curves"], s["constraints"], s["dimensions"]])
}

#[test]
fn a_3d_point_s_coordinates_written_as_expressions_follow_the_parameters() {
    let mut d = with_parameters();
    command(&mut d, json!({"cmd": "sketch3d.create"}));
    let point = command(
        &mut d,
        json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": ["w", 5, "h / 2"]}),
    );
    let p = id(&point["made"][0]);
    assert_eq!(point["inputs"]["at"], p.as_str(), "{point}");
    // x and z are held, y (a number) is free.
    let coordinates = point["coordinates"].as_array().unwrap();
    assert_eq!(coordinates.len(), 2, "{point}");
    assert_eq!(coordinates[0]["axis"], "x");
    assert_eq!(coordinates[0]["expression"], "w");
    assert_eq!(coordinates[0]["parameter"], "w");
    assert_eq!(coordinates[1]["axis"], "z");
    assert_eq!(coordinates[1]["expression"], "h / 2");
    // The origin point, attached to the sketch's origin, is made with it.
    assert_eq!(point["entities"].as_array().unwrap().len(), 2, "{point}");
    let s = sketch3d(&d);
    let start = at3(&s, &p);
    assert!(close(start[0], 60.0) && close(start[1], 5.0) && close(start[2], 20.0));
    let k = id(&coordinates[0]["dimension"]);
    let dim = dimension3(&s, &k);
    assert_eq!(dim["type"], "offset", "{dim}");
    assert_eq!(dim["axis"], "x", "{dim}");
    assert_eq!(dim["b"], p.as_str(), "{dim}");
    let origin = id(&dim["a"]);
    assert!(on_origin(&s, &origin), "{s}");

    set(&mut d, "w", 80.0);
    set(&mut d, "h", 10.0);
    let moved = at3(&sketch3d(&d), &p);
    assert!(close(moved[0], 80.0) && close(moved[2], 5.0), "{moved:?}");

    // A second expression point takes the same origin point.
    let more = command(
        &mut d,
        json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": [0, "w", 0]}),
    );
    assert_eq!(more["entities"].as_array().unwrap().len(), 1, "{more}");
    let k = id(&more["coordinates"][0]["dimension"]);
    assert_eq!(dimension3(&sketch3d(&d), &k)["a"], origin.as_str());

    // Numbers only: no dimension, the result as before.
    let plain = command(
        &mut d,
        json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": [1, 2, 3]}),
    );
    assert!(plain.get("coordinates").is_none(), "{plain}");
    assert_eq!(plain["dimensions"], json!([]), "{plain}");
}

#[test]
fn a_3d_line_s_end_written_as_expressions_is_the_point_the_line_ends_at() {
    let mut d = with_parameters();
    command(&mut d, json!({"cmd": "sketch3d.create"}));
    let line = command(
        &mut d,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0],
               "end": ["w", 10, 0], "start_source": "origin"}),
    );
    let end = id(&line["end"]);
    assert_eq!(line["inputs"]["end"], end.as_str(), "{line}");
    assert_eq!(line["inputs"]["start"], line["start"], "{line}");
    // The start attached to the origin is the origin point: no other one.
    let k = id(&line["coordinates"][0]["dimension"]);
    let s = sketch3d(&d);
    assert_eq!(dimension3(&s, &k)["a"], line["start"], "{s}");
    set(&mut d, "w", 25.0);
    let moved = at3(&sketch3d(&d), &end);
    assert!(close(moved[0], 25.0), "{moved:?}");

    // A spline's points and an arc's centre take them too.
    let spline = command(
        &mut d,
        json!({"cmd": "sketch3d.add_spline", "sketch": "F1",
               "points": [[0, 0, 5], [10, "h", 5], [20, 0, 5]]}),
    );
    let middle = id(&spline["inputs"]["points"][1]);
    assert_eq!(spline["spline_points"][1], middle.as_str(), "{spline}");
    assert_eq!(spline["coordinates"][0]["axis"], "y", "{spline}");
    set(&mut d, "h", 12.0);
    assert!(close(at3(&sketch3d(&d), &middle)[1], 12.0));
}

#[test]
fn places_without_a_point_refuse_expressions_in_words_and_change_nothing() {
    let mut d = with_parameters();
    command(&mut d, json!({"cmd": "sketch3d.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": [1, 1, 1]}),
    );
    let before = items3(&sketch3d(&d));
    let refusals = [
        (
            json!({"cmd": "sketch3d.add_arc", "sketch": "F1", "mode": "three_point",
                   "start": [0, 0, 0], "through": ["w", 5, 0], "end": [10, 0, 0]}),
            "through: the command makes no point of its own there",
        ),
        (
            json!({"cmd": "sketch3d.add_text", "sketch": "F1", "text": "A", "at": ["w", 0, 0],
                   "plane": "xy", "height": 5}),
            "at: the command makes no point of its own there",
        ),
        (
            json!({"cmd": "sketch3d.add_polygon", "sketch": "F1", "center": [0, 0, 0],
                   "vertex": ["w", 0, 0], "sides": 6, "inscribed": false, "plane": "xy"}),
            "vertex: the command makes no point of its own there",
        ),
        (
            json!({"cmd": "sketch3d.add_rectangle", "sketch": "F1", "mode": "three_point",
                   "a": [0, 0, 0], "b": [10, 0, 0], "c": [0, "h", 0], "plane": "xy"}),
            "c: the command makes no point of its own there",
        ),
        (
            json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": ["nope", 0, 0]}),
            "at.x",
        ),
        (
            json!({"cmd": "sketch3d.add_point", "sketch": "F1", "at": "p1"}),
            "\"at\" is a position [x, y, z], not a point id",
        ),
    ];
    for (refused, expected) in refusals {
        let error = rejected(&mut d, refused.clone());
        assert!(error.contains(expected), "{refused}: {error}");
        assert_eq!(items3(&sketch3d(&d)), before, "{refused}");
    }
}

#[test]
fn a_move_and_a_scale_refuse_expressions_in_words() {
    let mut d = with_parameters();
    command(&mut d, json!({"cmd": "sketch.create"}));
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    let c = id(&line["made"][0]);
    let refusals = [
        (
            json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c], "by": ["w", 0]}),
            "\"by\" of a move is read once, so it cannot follow parameters: give numbers",
        ),
        (
            json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c], "from": [0, 0],
                   "to": ["w", 0]}),
            "\"to\" of a move is read once",
        ),
        (
            json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c],
                   "rotate": {"center": [0, "h"], "angle": 1.0}}),
            "\"rotate.center\" of a move is read once",
        ),
        (
            json!({"cmd": "sketch.scale", "sketch": "F1", "factor": 2, "center": ["w", 0]}),
            "\"center\" of a scale is read once",
        ),
        (
            json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c], "by": [1, 2, 3]}),
            "\"by\" is an offset [dx, dy] of numbers",
        ),
    ];
    let before = query(&d, json!({"query": "sketch", "uid": "F1"}))["entities"].clone();
    for (refused, expected) in refusals {
        let error = rejected(&mut d, refused.clone());
        assert!(error.contains(expected), "{refused}: {error}");
        let after = query(&d, json!({"query": "sketch", "uid": "F1"}))["entities"].clone();
        assert_eq!(after, before, "{refused}");
    }
    // Numbers move it as before.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c], "by": [0, 5]}),
    );
}

#[test]
fn a_circular_pattern_s_centre_written_as_expressions_follows_them() {
    let mut d = with_parameters();
    command(&mut d, json!({"cmd": "sketch.create"}));
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [10, 0], "end": [20, 0]}),
    );
    let c = id(&line["made"][0]);
    let pattern = command(
        &mut d,
        json!({"cmd": "sketch.circular_pattern", "sketch": "F1", "entities": [c],
               "center": ["w/2", 0], "count": 3}),
    );
    let center = id(&pattern["inputs"]["center"]);
    // Both coordinates are held: the number as a length of its own.
    let coordinates = pattern["coordinates"].as_array().unwrap();
    assert_eq!(coordinates.len(), 2, "{pattern}");
    assert_eq!(coordinates[0]["expression"], "w/2");
    assert_eq!(coordinates[1]["expression"], "0 mm");
    let s = query(&d, json!({"query": "sketch", "uid": "F1"}));
    let entity = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == center.as_str())
        .unwrap()
        .clone();
    assert_ne!(
        entity["fixed"], true,
        "held by dimensions, not fixed: {entity}"
    );
    set(&mut d, "w", 80.0);
    let s = query(&d, json!({"query": "sketch", "uid": "F1"}));
    let moved = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == center.as_str())
        .unwrap()["at"]
        .clone();
    assert!(
        close(moved[0].as_f64().unwrap(), 40.0) && close(moved[1].as_f64().unwrap(), 0.0),
        "{moved}"
    );
    // A centre given by numbers is fixed as before, with no dimensions.
    let other = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 50], "end": [5, 50]}),
    );
    let plain = command(
        &mut d,
        json!({"cmd": "sketch.circular_pattern", "sketch": "F1", "entities": [other["made"][0]],
               "center": [0, 30], "count": 2}),
    );
    assert!(plain.get("coordinates").is_none(), "{plain}");
}
