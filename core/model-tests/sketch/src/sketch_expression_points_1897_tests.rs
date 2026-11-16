// SPDX-License-Identifier: MIT
//! Point coordinates written as expressions of parameters (mitcad#1897):
//! the point is drawn where the expression is now and held there by a
//! signed distance from the sketch's origin point, so that changing the
//! parameter moves it; number coordinates stay free, existing points are
//! as before. A drawing command's `inputs` names the point at each point
//! input as written, a clockwise arc's ends too. Expressions that cannot be
//! kept leave the sketch and the parameters as they were.

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

fn sketch(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "sketch", "uid": "F1"}))
}

/// A document with the parameters w = 60 and h = 40 and an empty Sketch1
/// (F1).
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
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn entity(s: &Value, id: &str) -> Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn point_at(s: &Value, id: &str) -> [f64; 2] {
    let e = entity(s, id);
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

fn dimension(s: &Value, id: &str) -> Value {
    s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

/// What a refused edit must leave as it was: the sketch's items.
fn items(s: &Value) -> Value {
    json!([s["entities"], s["constraints"], s["dimensions"]])
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn assert_at(s: &Value, id: &str, expected: [f64; 2]) {
    let [x, y] = point_at(s, id);
    assert!(
        close(x, expected[0]) && close(y, expected[1]),
        "{id} is at ({x}, {y}), not {expected:?}: {s}"
    );
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

#[test]
fn a_coordinate_written_as_a_parameter_follows_it_and_a_number_stays_free() {
    let mut d = with_parameters();
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": ["w", 5]}),
    );
    let (start, end) = (id(&line["inputs"]["start"]), id(&line["inputs"]["end"]));
    // One dimension, x of the end: it uses w itself.
    let coordinates = line["coordinates"].as_array().unwrap();
    assert_eq!(coordinates.len(), 1, "{line}");
    let held = &coordinates[0];
    assert_eq!(held["point"], end.as_str(), "{line}");
    assert_eq!(held["axis"], "x", "{line}");
    assert_eq!(held["expression"], "w", "{line}");
    assert_eq!(held["parameter"], "w", "{line}");
    assert_eq!(line["parameters"], json!([]), "{line}");
    let k = id(&held["dimension"]);
    assert!(
        line["dimensions"].as_array().unwrap().contains(&json!(k)),
        "{line}"
    );
    let s = sketch(&d);
    assert_at(&s, &end, [60.0, 5.0]);
    assert_at(&s, &start, [0.0, 0.0]);
    // A signed horizontal distance from the origin point, a fixed point of
    // its own at [0, 0], not the line's start.
    let dim = dimension(&s, &k);
    assert_eq!(dim["type"], "horizontal_distance", "{dim}");
    assert_eq!(dim["signed"], true, "{dim}");
    assert_eq!(dim["b"], end.as_str(), "{dim}");
    let origin = id(&dim["a"]);
    assert_ne!(origin, start);
    assert_at(&s, &origin, [0.0, 0.0]);
    assert_eq!(entity(&s, &origin)["fixed"], true, "{s}");

    // Changing w moves the end; its y, a number, and the start are free.
    set(&mut d, "w", 80.0);
    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    assert_at(&s, &end, [80.0, 5.0]);
    assert_at(&s, &start, [0.0, 0.0]);
    // The start's two and the end's y degrees of freedom are left.
    assert_eq!(s["dof"], 3, "{s}");

    // A second expression point takes the same origin point.
    let more = command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": ["w/2", "h"]}),
    );
    let s = sketch(&d);
    for held in more["coordinates"].as_array().unwrap() {
        assert_eq!(dimension(&s, &id(&held["dimension"]))["a"], origin.as_str());
    }
}

#[test]
fn expressions_in_both_coordinates_get_parameters_of_their_own() {
    let mut d = with_parameters();
    let point = command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": ["w/2", "h + 3"]}),
    );
    let p = id(&point["inputs"]["at"]);
    let coordinates = point["coordinates"].as_array().unwrap();
    assert_eq!(coordinates.len(), 2, "{point}");
    assert_eq!(coordinates[0]["axis"], "x");
    assert_eq!(coordinates[0]["expression"], "w/2");
    assert_eq!(coordinates[1]["axis"], "y");
    assert_eq!(coordinates[1]["expression"], "h + 3");
    // Each expression is a parameter owned by the sketch.
    let made = point["parameters"].as_array().unwrap();
    assert_eq!(made.len(), 2, "{point}");
    for held in coordinates {
        assert!(made.contains(&held["parameter"]), "{point}");
    }
    let s = sketch(&d);
    assert_at(&s, &p, [30.0, 43.0]);
    assert_eq!(
        dimension(&s, &id(&coordinates[1]["dimension"]))["type"],
        "vertical_distance"
    );
    set(&mut d, "w", 100.0);
    set(&mut d, "h", 10.0);
    assert_at(&sketch(&d), &p, [50.0, 13.0]);
    // A negative value puts the point on the other side of the origin.
    set(&mut d, "w", -20.0);
    assert_at(&sketch(&d), &p, [-10.0, 13.0]);
}

#[test]
fn expression_coordinates_are_read_in_the_documents_length_unit() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "set_units", "length": "in"}));
    command(&mut d, json!({"cmd": "sketch.create"}));
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": ["2", 10]}),
    );
    // A JSON number stays millimetres; the text is 2 in.
    assert_at(&sketch(&d), &id(&line["inputs"]["end"]), [50.8, 10.0]);
}

#[test]
fn existing_points_and_numbers_are_as_before_and_inputs_name_their_points() {
    let mut d = with_parameters();
    let first = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    assert!(first.get("coordinates").is_none(), "{first}");
    let end = id(&first["inputs"]["end"]);
    let second = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": end, "end": [10, 10]}),
    );
    // The existing point is the input's point; no dimension was added.
    assert_eq!(second["inputs"]["start"], end.as_str(), "{second}");
    assert_eq!(second["dimensions"], json!([]), "{second}");
    assert!(second.get("coordinates").is_none(), "{second}");
    // A spline's points as a list, in order.
    let spline = command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1",
               "points": [[20, 0], [25, "h"], [30, 0]]}),
    );
    let points = spline["inputs"]["points"].as_array().unwrap().clone();
    assert_eq!(points.len(), 3, "{spline}");
    let s = sketch(&d);
    assert_at(&s, &id(&points[0]), [20.0, 0.0]);
    assert_at(&s, &id(&points[1]), [25.0, 40.0]);
    assert_at(&s, &id(&points[2]), [30.0, 0.0]);
    // A rectangle's corners as written.
    let rectangle = command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point",
               "a": [100, 0], "b": [120, 10]}),
    );
    let s = sketch(&d);
    assert_at(&s, &id(&rectangle["inputs"]["a"]), [100.0, 0.0]);
    assert_at(&s, &id(&rectangle["inputs"]["b"]), [120.0, 10.0]);
}

#[test]
fn a_clockwise_arcs_inputs_are_its_ends_as_written() {
    let mut d = with_parameters();
    // From (60, 40) through (30, 30) to (0, 40): clockwise, so the arc is
    // stored counter-clockwise from the end written to the start written.
    let arc = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": [60, 40], "through": [30, 30], "end": [0, "h"]}),
    );
    let (start, end) = (id(&arc["inputs"]["start"]), id(&arc["inputs"]["end"]));
    assert!(arc["inputs"]["through"].is_null(), "{arc}");
    let s = sketch(&d);
    assert_at(&s, &start, [60.0, 40.0]);
    assert_at(&s, &end, [0.0, 40.0]);
    let curve = entity(&s, &id(&arc["made"][0]));
    assert_eq!(curve["start"], end.as_str(), "{curve}");
    assert_eq!(curve["end"], start.as_str(), "{curve}");
    // The end written as h follows it.
    assert_eq!(arc["coordinates"][0]["point"], end.as_str(), "{arc}");
    set(&mut d, "h", 45.0);
    let s = sketch(&d);
    assert!(close(point_at(&s, &end)[1], 45.0), "{s}");
}

#[test]
fn expressions_that_cannot_be_kept_change_nothing() {
    let mut d = with_parameters();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    let unchanged = |d: &Document<MockKernel>| {
        (
            items(&sketch(d)),
            query(d, json!({"query": "parameters"})),
            query(d, json!({"query": "document"}))["undo_depth"].clone(),
        )
    };
    let before = unchanged(&d);
    for (refused, says) in [
        // No such parameter, and no expression.
        (
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0],
                   "end": ["nope", 5]}),
            "end.x: parameter nope does not exist",
        ),
        (
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0],
                   "end": [5, "w +"]}),
            "end.y",
        ),
        // A position where the arc makes no point.
        (
            json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
                   "start": [60, 40], "through": ["w/2", 30], "end": [0, 40]}),
            "through: the command makes no point of its own there",
        ),
        // A fixed point does not move: its coordinates cannot follow w.
        (
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": ["w", 0],
                   "fixed": true}),
            "",
        ),
        // "at" of a point is a position.
        (
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": "p1"}),
            "\"at\" is a position",
        ),
    ] {
        let error = rejected(&mut d, refused.clone());
        assert!(error.contains(says), "{refused}: {error}");
        assert_eq!(unchanged(&d), before, "{refused}: {error}");
    }
}

#[test]
fn constraints_that_contradict_or_repeat_expression_coordinates_are_refused() {
    let mut d = with_parameters();
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, "h"],
               "end": [10, "h + 3"]}),
    );
    let c = id(&line["made"][0]);
    let before = items(&sketch(&d));
    // Horizontal would need h = h + 3.
    rejected(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": c}}),
    );
    assert_eq!(items(&sketch(&d)), before);
    // Both ends at h: horizontal follows from the two dimensions.
    let level = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [20, "h"],
               "end": [30, "h"]}),
    );
    let level = id(&level["made"][0]);
    let before = items(&sketch(&d));
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": level}}),
    );
    assert!(error.contains("over-constrain"), "{error}");
    assert_eq!(items(&sketch(&d)), before);
}
