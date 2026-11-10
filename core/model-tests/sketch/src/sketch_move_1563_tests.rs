// SPDX-License-Identifier: MIT
//! Sketch Move/Copy (mitcad#1563): `sketch.move` Point to Point (`from`
//! and `to`), Rotate about the entities' middle when no pivot is given, and
//! a rotation that turns the constraints holding the moved geometry to the
//! sketch's axes, instead of collapsing the geometry onto them.

use std::f64::consts::PI;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

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

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
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

/// Where a point is.
fn at(s: &Value, point: &str) -> [f64; 2] {
    let p = &entity(s, point)["at"];
    [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]
}

/// The point at a place.
fn point_at(s: &Value, place: [f64; 2]) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["at"].as_array().is_some_and(|p| {
                (p[0].as_f64().unwrap() - place[0]).abs() <= 1e-9
                    && (p[1].as_f64().unwrap() - place[1]).abs() <= 1e-9
            })
        })
        .unwrap_or_else(|| panic!("no point at {place:?} in {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A line's start and end point ids.
fn ends(s: &Value, line: &str) -> [String; 2] {
    let e = entity(s, line);
    [
        e["start"].as_str().unwrap().to_owned(),
        e["end"].as_str().unwrap().to_owned(),
    ]
}

/// The baseline tolerance of geometric comparisons.
fn assert_near(a: [f64; 2], b: [f64; 2], what: &str) {
    assert!(
        (a[0] - b[0]).abs() <= 1e-6 && (a[1] - b[1]).abs() <= 1e-6,
        "{what}: {a:?}, expected {b:?}"
    );
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn ids(result: &Value, field: &str) -> Vec<String> {
    result[field]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn curves(result: &Value) -> Vec<String> {
    ids(result, "entities")
        .into_iter()
        .filter(|id| id.starts_with('c'))
        .collect()
}

fn types(s: &Value, field: &str) -> Vec<String> {
    s[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_str().unwrap().to_owned())
        .collect()
}

fn count(list: &[String], kind: &str) -> usize {
    list.iter().filter(|t| t.as_str() == kind).count()
}

#[test]
fn point_to_point_moves_by_the_target_less_the_origin() {
    let mut d = with_sketch();
    let rectangle = command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point",
               "a": [-30, -20], "b": [-20, -10]}),
    );
    let lines = curves(&rectangle);
    assert_eq!(lines.len(), 4, "{rectangle}");
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [20, -10], "radius": 3}),
    );
    let s = sketch(&d);
    let corner = point_at(&s, [-30.0, -20.0]);
    let opposite = point_at(&s, [-20.0, -10.0]);
    let center = point_at(&s, [20.0, -10.0]);

    // From the corner to the circle's center, both existing points.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": lines,
               "from": corner, "to": center}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &corner), [20.0, -10.0], "the corner");
    assert_near(at(&s, &opposite), [30.0, 0.0], "the opposite corner");
    assert_near(at(&s, &center), [20.0, -10.0], "the circle's center stays");

    // Positions, as a snap to a curve's middle gives them, and a copy.
    let copy = command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": lines,
               "from": [25, -10], "to": [25, 10], "copy": true}),
    );
    let made = ids(&copy, "made");
    assert_eq!(made.len(), 4, "{copy}");
    let s = sketch(&d);
    assert_near(at(&s, &corner), [20.0, -10.0], "the original stays");
    let [a, b] = ends(&s, &made[0]);
    let moved = [at(&s, &a), at(&s, &b)];
    assert!(
        moved.iter().all(|p| p[1] >= 10.0 - 1e-6),
        "the copy is 20 up: {moved:?}"
    );

    // Point to Point takes both points and nothing else.
    let only_from = rejected(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": lines, "from": corner}),
    );
    assert!(only_from.contains("\"from\" and \"to\""), "{only_from}");
    let both = rejected(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": lines,
               "by": [1, 0], "from": corner, "to": center}),
    );
    assert!(both.contains("\"from\" and \"to\""), "{both}");
}

#[test]
fn rotate_without_a_pivot_turns_a_horizontal_line_about_its_middle() {
    let mut d = with_sketch();
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [-25, 10], "end": [-5, 10]}),
    );
    let c = curves(&line).remove(0);
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": c}}),
    );
    let s = sketch(&d);
    let [start, end] = ends(&s, &c);
    // The selection disappeared here (GitHub mitcad/Mitcad#212): the
    // horizontal constraint held both ends level as they were pulled one
    // above the other, and the line collapsed to a point; and without a
    // pivot it turned about the sketch's origin.
    let result = command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c],
               "rotate": {"angle": PI / 2.0}}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &start), [-15.0, 0.0], "the start");
    assert_near(at(&s, &end), [-15.0, 20.0], "the end");
    assert_eq!(types(&s, "constraints"), vec!["vertical".to_owned()], "{s}");
    assert!(result.get("notes").is_none(), "{result}");

    // A half turn keeps it vertical, now about the same middle.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c],
               "rotate": {"angle": PI}}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &start), [-15.0, 20.0], "the start, half turned");
    assert_near(at(&s, &end), [-15.0, 0.0], "the end, half turned");
    assert_eq!(types(&s, "constraints"), vec!["vertical".to_owned()]);

    // A pivot given still turns about it.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [c],
               "rotate": {"center": [0, 0], "angle": -PI / 2.0}}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &start), [20.0, 15.0], "the start about the origin");
    assert_near(at(&s, &end), [0.0, 15.0], "the end about the origin");
    assert_eq!(types(&s, "constraints"), vec!["horizontal".to_owned()]);
}

#[test]
fn a_rotated_rectangle_keeps_its_shape_and_dimensions() {
    let mut d = with_sketch();
    let rectangle = command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point",
               "a": [0, 0], "b": [20, 10]}),
    );
    let lines = curves(&rectangle);
    let s = sketch(&d);
    let corners = [
        point_at(&s, [0.0, 0.0]),
        point_at(&s, [20.0, 0.0]),
        point_at(&s, [20.0, 10.0]),
        point_at(&s, [0.0, 10.0]),
    ];
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "horizontal_distance", "a": corners[0], "b": corners[1]},
               "value": 20}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "vertical_distance", "a": corners[1], "b": corners[2]},
               "value": 10}),
    );

    // A quarter turn about its middle (10, 5): the horizontal sides and
    // the width become vertical, the vertical ones horizontal.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": lines,
               "rotate": {"angle": PI / 2.0}}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &corners[0]), [15.0, -5.0], "the first corner");
    assert_near(at(&s, &corners[2]), [5.0, 15.0], "the third corner");
    let constraints = types(&s, "constraints");
    assert_eq!(count(&constraints, "horizontal"), 2, "{constraints:?}");
    assert_eq!(count(&constraints, "vertical"), 2, "{constraints:?}");
    let dimensions = types(&s, "dimensions");
    assert_eq!(
        dimensions,
        vec![
            "vertical_distance".to_owned(),
            "horizontal_distance".to_owned()
        ],
        "{s}"
    );

    // 30 degrees: no longer on the axes, the sides are held parallel and
    // perpendicular instead, the distances between corners level or plumb
    // become aligned ones; the shape and its size stay.
    let result = command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": lines,
               "rotate": {"angle": PI / 6.0}}),
    );
    let notes = result["notes"].to_string();
    assert!(notes.contains("parallel or perpendicular"), "{result}");
    assert!(notes.contains("aligned distance"), "{result}");
    let s = sketch(&d);
    let p: Vec<[f64; 2]> = corners.iter().map(|c| at(&s, c)).collect();
    for (i, side) in [20.0, 10.0, 20.0, 10.0].iter().enumerate() {
        let length = distance(p[i], p[(i + 1) % 4]);
        assert!((length - side).abs() <= 1e-6, "side {i}: {length}");
    }
    assert!(
        (distance(p[0], p[2]) - 500.0_f64.sqrt()).abs() <= 1e-6,
        "still a rectangle: {p:?}"
    );
    let middle = [(p[0][0] + p[2][0]) / 2.0, (p[0][1] + p[2][1]) / 2.0];
    assert_near(middle, [10.0, 5.0], "turned about its middle");
    let constraints = types(&s, "constraints");
    assert_eq!(count(&constraints, "horizontal"), 0, "{constraints:?}");
    assert_eq!(count(&constraints, "vertical"), 0, "{constraints:?}");
    assert_eq!(count(&constraints, "parallel"), 1, "{constraints:?}");
    assert_eq!(count(&constraints, "perpendicular"), 2, "{constraints:?}");
    assert_eq!(
        types(&s, "dimensions"),
        vec!["distance".to_owned(), "distance".to_owned()],
        "{s}"
    );
}

#[test]
fn a_half_turn_measures_a_signed_distance_from_its_other_point() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [5, 5]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [15, 5]}),
    );
    let s = sketch(&d);
    let (a, b) = (point_at(&s, [5.0, 5.0]), point_at(&s, [15.0, 5.0]));
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "horizontal_distance", "a": a, "b": b, "signed": true},
               "value": 10}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [a, b],
               "rotate": {"angle": PI}}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &a), [15.0, 5.0], "the first point");
    assert_near(at(&s, &b), [5.0, 5.0], "the second point");
    let dimension = &s["dimensions"][0];
    assert_eq!(dimension["a"], json!(b), "{dimension}");
    assert_eq!(dimension["b"], json!(a), "{dimension}");
}
