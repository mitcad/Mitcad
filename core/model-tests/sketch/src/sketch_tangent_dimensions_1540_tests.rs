// SPDX-License-Identifier: MIT
//! Tangent dimensions between circles (mitcad#1540): the gap between two
//! circles or arcs at their nearest sides (`tangent_circle_distance`,
//! outside each other or one inside the other) and the gap from a circle's
//! nearest side to a point, another circle's centre among them
//! (`tangent_point_distance`). A driving one moves the geometry to its
//! value; a driven one only measures.

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

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn entity<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
}

fn dimension<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
}

/// A circle's centre in the solved sketch.
fn centre(s: &Value, circle: &str) -> [f64; 2] {
    let g = &entity(s, circle)["geometry"]["center"];
    [g[0].as_f64().unwrap(), g[1].as_f64().unwrap()]
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= 1e-6,
        "{what}: {actual} vs {expected}"
    );
}

/// A sketch with a circle of `outer` diameter about the fixed origin and a
/// circle of `inner` diameter about a free centre at `at`, both sizes
/// dimensioned, the free centre held level with the fixed one. Returns the
/// document, the two circles and the free circle's centre point.
fn two_circles(
    outer: f64,
    inner: f64,
    at: [f64; 2],
) -> (Document<MockKernel>, String, String, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let a = command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": outer}),
    );
    let a = a["curves"][0].as_str().unwrap().to_owned();
    let b = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": at, "radius": inner / 2.0}),
    );
    let b = b["made"][0].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "diameter", "curve": b}, "value": inner}),
    );
    let s = sketch(&d);
    let pa = entity(&s, &a)["center"].as_str().unwrap().to_owned();
    let pb = entity(&s, &b)["center"].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal_points", "a": pa, "b": pb}}),
    );
    (d, a, b, pb)
}

#[test]
fn tangent_to_tangent_between_two_circles_solves_to_the_typed_value() {
    // Ø20 at the origin, Ø10 at 30: the gap 30 - 10 - 5 = 15 as drawn.
    let (mut d, a, b, _) = two_circles(20.0, 10.0, [30.0, 0.0]);
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_circle_distance", "a": a, "b": b},
               "value": 12}),
    );
    let k = dim["dimensions"][0].as_str().unwrap();
    let s = sketch(&d);
    assert_close(
        distance(centre(&s, &a), centre(&s, &b)),
        27.0,
        "10 + 12 + 5",
    );
    assert_close(
        dimension(&s, k)["measured"].as_f64().unwrap(),
        12.0,
        "measured",
    );
    // Outside is the default and is not written.
    assert_eq!(dimension(&s, k)["type"], "tangent_circle_distance", "{s}");
    assert!(dimension(&s, k).get("side").is_none(), "{s}");
    // A new value moves the free circle again.
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": k, "value": "4.25 mm"}),
    );
    let s = sketch(&d);
    assert_close(
        distance(centre(&s, &a), centre(&s, &b)),
        19.25,
        "10 + 4.25 + 5",
    );
    // A circle with itself is refused.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_circle_distance", "a": a, "b": a},
               "value": 3}),
    );
    assert!(!error.is_empty());
}

#[test]
fn tangent_to_centre_between_two_circles_solves_to_the_typed_value() {
    // From the Ø20 circle's side to the Ø10 circle's centre: 30 - 10 = 20
    // as drawn.
    let (mut d, a, b, pb) = two_circles(20.0, 10.0, [30.0, 0.0]);
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_point_distance", "circle": a, "point": pb},
               "value": 17.5}),
    );
    let k = dim["dimensions"][0].as_str().unwrap();
    let s = sketch(&d);
    assert_close(distance(centre(&s, &a), centre(&s, &b)), 27.5, "10 + 17.5");
    assert_close(
        dimension(&s, k)["measured"].as_f64().unwrap(),
        17.5,
        "measured",
    );
    assert_eq!(dimension(&s, k)["type"], "tangent_point_distance", "{s}");
    assert_eq!(dimension(&s, k)["point"], pb.as_str(), "{s}");
}

#[test]
fn inside_gaps_measure_from_the_inner_side() {
    // Ø40 at the origin holding Ø10 at 6: the gap inside 20 - 5 - 6 = 9.
    let (mut d, a, b, pb) = two_circles(40.0, 10.0, [6.0, 0.0]);
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_circle_distance", "a": b, "b": a, "side": "inside"},
               "value": 3}),
    );
    let k = dim["dimensions"][0].as_str().unwrap();
    let s = sketch(&d);
    assert_close(distance(centre(&s, &a), centre(&s, &b)), 12.0, "20 - 5 - 3");
    assert_close(
        dimension(&s, k)["measured"].as_f64().unwrap(),
        3.0,
        "measured",
    );
    assert_eq!(dimension(&s, k)["side"], "inside", "{s}");
    // The free centre from the outer circle's side, inside it: 20 - 12.
    let driven = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_point_distance", "circle": a, "point": pb,
                             "side": "inside"},
               "driven": true}),
    );
    let s = sketch(&d);
    let driven = driven["dimensions"][0].as_str().unwrap();
    assert_close(
        dimension(&s, driven)["measured"].as_f64().unwrap(),
        8.0,
        "20 - 12",
    );
}

#[test]
fn a_driven_tangent_dimension_only_measures() {
    let (mut d, a, b, pb) = two_circles(20.0, 10.0, [30.0, 0.0]);
    let before = centre(&sketch(&d), &b);
    let gap = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_circle_distance", "a": a, "b": b},
               "driven": true}),
    );
    let to_centre = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_point_distance", "circle": a, "point": pb},
               "driven": true}),
    );
    let s = sketch(&d);
    let after = centre(&s, &b);
    assert_close(distance(before, after), 0.0, "nothing moved");
    let gap = gap["dimensions"][0].as_str().unwrap();
    let to_centre = to_centre["dimensions"][0].as_str().unwrap();
    assert_eq!(dimension(&s, gap)["driven"], true, "{s}");
    assert_close(
        dimension(&s, gap)["measured"].as_f64().unwrap(),
        15.0,
        "30 - 10 - 5",
    );
    assert_close(
        dimension(&s, to_centre)["measured"].as_f64().unwrap(),
        20.0,
        "30 - 10",
    );
    // Driving the free circle elsewhere: the driven ones follow.
    let driving = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_circle_distance", "a": a, "b": b},
               "value": 8}),
    );
    assert!(driving["dimensions"][0].is_string(), "{driving}");
    let s = sketch(&d);
    assert_close(
        dimension(&s, gap)["measured"].as_f64().unwrap(),
        8.0,
        "follows",
    );
    assert_close(
        dimension(&s, to_centre)["measured"].as_f64().unwrap(),
        13.0,
        "8 + 5",
    );
}
