// SPDX-License-Identifier: MIT
//! Tangent dimensions to a circle's or arc's far side (mitcad#2451): with
//! `"side": "far"` a `tangent_distance` measures from the line to the far
//! side, the centre's distance plus the radius (the top of an arc above a
//! line), a `tangent_point_distance` from the point to the far side, and a
//! `tangent_circle_distance` between the outer tangents (`near_far`: from
//! `a`'s near side to `b`'s far side). Without a side, the near side as
//! before.

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

/// The first new entity of a command's result whose id starts with
/// `prefix` (`c` a curve, `p` a point).
fn made(result: &Value, prefix: &str) -> String {
    result["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_owned())
        .find(|e| e.starts_with(prefix))
        .unwrap_or_else(|| panic!("no {prefix} in {result}"))
}

/// A circle's or arc's centre in the solved sketch.
fn centre(s: &Value, curve: &str) -> [f64; 2] {
    let g = &entity(s, curve)["geometry"]["center"];
    [g[0].as_f64().unwrap(), g[1].as_f64().unwrap()]
}

fn radius(s: &Value, curve: &str) -> f64 {
    entity(s, curve)["geometry"]["radius"].as_f64().unwrap()
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

/// A sketch with a fixed line along the x axis and an arc of radius 10
/// (its size dimensioned) above it, the upper half of a circle about
/// (0, 15): its top 25 above the line. Returns the document, the line and
/// the arc.
fn arc_above_a_line() -> (Document<MockKernel>, String, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [-50, 0], "end": [50, 0]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": line["entities"]}),
    );
    let line = made(&line, "c");
    let arc = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": [0, 15],
               "start": [10, 15], "end": [-10, 15]}),
    );
    let arc = made(&arc, "c");
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "radius", "curve": arc}, "value": 10}),
    );
    (d, line, arc)
}

#[test]
fn a_far_tangent_distance_drives_the_top_of_an_arc_from_a_line() {
    let (mut d, line, arc) = arc_above_a_line();
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_distance", "circle": arc, "line": line,
                             "side": "far"},
               "value": 30}),
    );
    let k = dim["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    // The arc's top point (its centre plus the radius upwards) is 30 from
    // the line, which stays on the x axis.
    let c = centre(&s, &arc);
    assert_close(radius(&s, &arc), 10.0, "the radius");
    assert_close(c[1] + radius(&s, &arc), 30.0, "the arc's top from the line");
    assert_close(c[1], 20.0, "the centre 30 - 10 above the line");
    assert_close(
        dimension(&s, &k)["measured"].as_f64().unwrap(),
        30.0,
        "measured",
    );
    // The side is written, and kept by the sketch query.
    assert_eq!(dimension(&s, &k)["side"], "far", "{s}");
    // A new value moves the arc again.
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": k, "value": "42.5 mm"}),
    );
    let s = sketch(&d);
    assert_close(centre(&s, &arc)[1], 32.5, "42.5 - 10");
}

#[test]
fn a_near_tangent_distance_stays_the_gap_to_the_near_side() {
    // As before mitcad#2451: the centre's distance minus the radius, and no
    // side written.
    let (mut d, line, arc) = arc_above_a_line();
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_distance", "circle": arc, "line": line},
               "value": 30}),
    );
    let k = dim["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert_close(
        centre(&s, &arc)[1],
        40.0,
        "the centre 30 + 10 above the line",
    );
    assert_close(
        dimension(&s, &k)["measured"].as_f64().unwrap(),
        30.0,
        "measured",
    );
    assert!(dimension(&s, &k).get("side").is_none(), "{s}");
    // "outside", the near side, is the default and not written either.
    let (mut d, line, arc) = arc_above_a_line();
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_distance", "circle": arc, "line": line,
                             "side": "outside"},
               "driven": true}),
    );
    let k = dim["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert!(dimension(&s, &k).get("side").is_none(), "{s}");
    assert_close(
        dimension(&s, &k)["measured"].as_f64().unwrap(),
        5.0,
        "15 - 10",
    );
}

#[test]
fn a_driven_far_tangent_distance_only_measures() {
    let (mut d, line, arc) = arc_above_a_line();
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_distance", "circle": arc, "line": line,
                             "side": "far"},
               "driven": true}),
    );
    let k = dim["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert_close(centre(&s, &arc)[1], 15.0, "the arc stays");
    assert_close(
        dimension(&s, &k)["measured"].as_f64().unwrap(),
        25.0,
        "15 + 10",
    );
}

#[test]
fn far_sides_of_circles_to_a_point_and_between_circles() {
    // A Ø20 circle with a fixed centre at the origin, a free Ø10 one at 30
    // held level with it, a free point above the origin at 18.
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let a = command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 20}),
    );
    let a = a["curves"][0].as_str().unwrap().to_owned();
    let b = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [30, 0], "radius": 5}),
    );
    let b = b["made"][0].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "diameter", "curve": b}, "value": 10}),
    );
    let s = sketch(&d);
    let pa = entity(&s, &a)["center"].as_str().unwrap().to_owned();
    let pb = entity(&s, &b)["center"].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal_points", "a": pa, "b": pb}}),
    );
    // Between the outer tangents: 30 + 10 + 5 = 45 as drawn; 50 moves the
    // free circle to 35.
    let outer = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_circle_distance", "a": a, "b": b, "side": "far"},
               "driven": true}),
    );
    let outer = outer["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert_close(
        dimension(&s, &outer)["measured"].as_f64().unwrap(),
        45.0,
        "30 + 10 + 5",
    );
    assert_eq!(dimension(&s, &outer)["side"], "far", "{s}");
    let near_far = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_circle_distance", "a": a, "b": b,
                             "side": "near_far"},
               "value": 30}),
    );
    let near_far = near_far["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    // From the Ø20's near side to the Ø10's far side: d - 10 + 5 = 30.
    assert_close(
        distance(centre(&s, &a), centre(&s, &b)),
        35.0,
        "30 + 10 - 5",
    );
    assert_close(
        dimension(&s, &outer)["measured"].as_f64().unwrap(),
        50.0,
        "35 + 10 + 5",
    );
    assert_eq!(dimension(&s, &near_far)["side"], "near_far", "{s}");
    // A point 18 above the fixed centre: 28 from the far side; 33 moves it
    // to 23.
    let p = command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 18]}),
    );
    let p = made(&p, "p");
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "vertical_points", "a": pa, "b": p}}),
    );
    let to_point = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_point_distance", "circle": a, "point": p,
                             "side": "far"},
               "value": 33}),
    );
    let to_point = to_point["dimensions"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    let at = &entity(&s, &p)["at"];
    let at = [at[0].as_f64().unwrap(), at[1].as_f64().unwrap()];
    assert_close(distance(centre(&s, &a), at), 23.0, "33 - 10");
    assert_close(
        dimension(&s, &to_point)["measured"].as_f64().unwrap(),
        33.0,
        "measured",
    );
}

#[test]
fn sides_a_tangent_dimension_cannot_have_are_refused() {
    let (mut d, line, arc) = arc_above_a_line();
    // A line has no inside, and no second circle.
    for side in ["inside", "near_far"] {
        let message = rejected(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": {"type": "tangent_distance", "circle": arc, "line": line,
                                 "side": side},
                   "value": 30}),
        );
        assert!(message.contains("side outside or far"), "{side}: {message}");
    }
    let s = sketch(&d);
    let p = entity(&s, &line)["start"].as_str().unwrap().to_owned();
    let message = rejected(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "tangent_point_distance", "circle": arc, "point": p,
                             "side": "near_far"},
               "driven": true}),
    );
    assert!(message.contains("side outside, inside or far"), "{message}");
}
