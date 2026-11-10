// SPDX-License-Identifier: MIT
//! The line tool's arcs and the Midpoint Line (mitcad#1393): `sketch.arc`
//! with `normal` leaves the end of a line or a circular arc square to it
//! and stays square when its end is dragged; `sketch.add_line` with
//! `midpoint` makes a line symmetric about a point held at its middle.

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

fn entities_of(s: &Value, kind: &str) -> Vec<Value> {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == kind)
        .cloned()
        .collect()
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    xy(&entity(s, id)["at"])
}

fn count(s: &Value, kind: &str) -> usize {
    s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == kind)
        .count()
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() <= 1e-6 && (a[1] - b[1]).abs() <= 1e-6
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn length(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

/// The arc `id`'s centre point's place.
fn center_of(s: &Value, id: &str) -> [f64; 2] {
    at(s, entity(s, id)["center"].as_str().unwrap())
}

/// The point at the other end of the arc `id` than `from`.
fn other_end(s: &Value, id: &str, from: &str) -> String {
    let arc = entity(s, id);
    let (a, b) = (arc["start"].as_str().unwrap(), arc["end"].as_str().unwrap());
    if a == from { b } else { a }.to_owned()
}

#[test]
fn a_normal_arc_leaves_a_line_square_and_stays_square() {
    let mut d = with_sketch();
    // c1 from p2 (0, 0) to p3 (10, 0).
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "normal", "from": "p3", "end": [15, 5]}),
    );
    let arc = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    // Its centre on the line, at (15, 0): square to the line at (10, 0),
    // its end where given.
    let geometry = &entity(&s, &arc)["geometry"];
    assert!(
        (geometry["radius"].as_f64().unwrap() - 5.0).abs() <= 1e-6,
        "{geometry}"
    );
    assert!(near(center_of(&s, &arc), [15.0, 0.0]), "{geometry}");
    let end = other_end(&s, &arc, "p3");
    assert!(near(at(&s, &end), [15.0, 5.0]), "{s}");
    assert_eq!(count(&s, "coincident"), 1, "{}", s["constraints"]);
    assert_eq!(count(&s, "tangent"), 0, "{}", s["constraints"]);
    // The end dragged away: the centre stays on the line, so the arc
    // still leaves it square.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": end, "to": [18, 9]}),
    );
    let s = sketch(&d);
    let (a, b) = (at(&s, "p2"), at(&s, "p3"));
    let c = center_of(&s, &arc);
    assert!(
        cross(sub(b, a), sub(c, b)).abs() <= 1e-6 * length(sub(b, a)) * length(sub(c, b)).max(1.0),
        "the centre {c:?} is off the line {a:?}-{b:?}"
    );
    // The arc still ends at the line's end.
    let arc_ends = [
        entity(&s, &arc)["start"].clone(),
        entity(&s, &arc)["end"].clone(),
    ];
    assert!(arc_ends.contains(&json!("p3")), "{s}");
}

#[test]
fn a_normal_arc_leaves_an_arc_square() {
    let mut d = with_sketch();
    // A quarter around (0, 0) from (10, 0) to (0, 10): it leaves (0, 10)
    // going -x.
    let quarter = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center",
               "center": [0, 0], "start": [10, 0], "end": [0, 10]}),
    );
    let quarter = quarter["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    let from = entity(&s, &quarter)["end"].as_str().unwrap().to_owned();
    assert!(near(at(&s, &from), [0.0, 10.0]), "{s}");
    let made = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "normal", "from": from, "end": [-5, 15]}),
    );
    let arc = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    // Centre (-5, 10) on the quarter's tangent at (0, 10), radius 5.
    assert!(near(center_of(&s, &arc), [-5.0, 10.0]), "{s}");
    // Held by a construction radius of the quarter that the new arc is
    // tangent to.
    let construction: Vec<Value> = entities_of(&s, "line")
        .into_iter()
        .filter(|l| l["construction"] == true && l["axis"].is_null())
        .collect();
    assert_eq!(construction.len(), 1, "{s}");
    assert_eq!(count(&s, "tangent"), 1, "{}", s["constraints"]);
    // Dragged, the two arcs still cross square at `from`: their radii there
    // are perpendicular.
    let end = other_end(&s, &arc, &from);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": end, "to": [-9, 13]}),
    );
    let s = sketch(&d);
    let p = at(&s, &from);
    let (old, new) = (sub(p, center_of(&s, &quarter)), sub(p, center_of(&s, &arc)));
    assert!(
        dot(old, new).abs() <= 1e-6 * length(old) * length(new),
        "the radii {old:?} and {new:?} at {p:?} are not square"
    );
}

#[test]
fn a_normal_arc_needs_a_line_or_an_arc_and_an_end_off_the_square() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    // Straight across from the line's end: a line, not an arc.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "normal", "from": "p3", "end": [10, 5]}),
    );
    assert!(error.contains("straight across"), "{error}");
    // From a spline's end.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1", "points": [[0, 0], [10, 5], [20, 0]]}),
    );
    let s = sketch(&d);
    let spline = entities_of(&s, "fitted_spline");
    assert_eq!(spline.len(), 1, "{s}");
    let last = spline[0]["points"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "normal", "from": last, "end": [30, 10]}),
    );
    assert!(error.contains("a line or a circular arc"), "{error}");
    // A tangent arc from it is still made.
    command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "tangent", "from": last, "end": [30, 10]}),
    );
}

/// The counter-clockwise sweep of the arc `id`, radians.
fn sweep_of(s: &Value, id: &str) -> f64 {
    let geometry = &entity(s, id)["geometry"];
    let (start, end) = (
        geometry["start_angle"].as_f64().unwrap(),
        geometry["end_angle"].as_f64().unwrap(),
    );
    (end - start).rem_euclid(std::f64::consts::TAU)
}

#[test]
fn a_normal_arc_of_more_than_half_a_turn_leaves_toward_the_side_it_is_drawn_to() {
    // (mitcad#1608) From the end (10, 0) of a line along +x, an arc of
    // radius 10 that sweeps 270 degrees upward round (20, 0) ends at
    // (20, -10), below the line: `toward` keeps it leaving upward.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "normal", "from": "p3",
               "end": [20, -10], "toward": [10, 5]}),
    );
    let arc = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert!(near(center_of(&s, &arc), [20.0, 0.0]), "{s}");
    let sweep = sweep_of(&s, &arc);
    assert!(
        (sweep - 1.5 * std::f64::consts::PI).abs() <= 1e-6,
        "sweep {sweep}: {s}"
    );
    // Clockwise from p3 (counter-clockwise to it): it leaves upward.
    assert_eq!(entity(&s, &arc)["end"], "p3", "{s}");
    // Without `toward` the end's side decides: the quarter that leaves
    // downward, as before.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "normal", "from": "p3", "end": [20, -10]}),
    );
    let arc = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    let sweep = sweep_of(&s, &arc);
    assert!(
        (sweep - 0.5 * std::f64::consts::PI).abs() <= 1e-6,
        "sweep {sweep}: {s}"
    );
    assert_eq!(entity(&s, &arc)["start"], "p3", "{s}");
    // Half a turn, the end on the line's own line: the side `toward` is on,
    // either way.
    for (toward, from_end) in [([10.0, 5.0], "end"), ([10.0, -5.0], "start")] {
        let mut d = with_sketch();
        command(
            &mut d,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
        );
        let made = command(
            &mut d,
            json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "normal", "from": "p3",
                   "end": [30, 0], "toward": toward}),
        );
        let arc = made["made"][0].as_str().unwrap().to_owned();
        let s = sketch(&d);
        assert!(near(center_of(&s, &arc), [20.0, 0.0]), "{s}");
        assert_eq!(entity(&s, &arc)[from_end], "p3", "{toward:?}: {s}");
    }
}

#[test]
fn a_midpoint_line_is_held_symmetric_about_its_middle() {
    let mut d = with_sketch();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "mode": "midpoint",
               "center": [0, 0], "end": [10, 5]}),
    );
    let line = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    let geometry = &entity(&s, &line)["geometry"];
    assert!(near(xy(&geometry["start"]), [-10.0, -5.0]), "{geometry}");
    assert!(near(xy(&geometry["end"]), [10.0, 5.0]), "{geometry}");
    assert_eq!(count(&s, "midpoint"), 1, "{}", s["constraints"]);
    // The middle is a point of its own, at the first click.
    let middle = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "midpoint")
        .unwrap()["point"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(near(at(&s, &middle), [0.0, 0.0]), "{s}");
    let end = entity(&s, &line)["end"].as_str().unwrap().to_owned();
    // An end dragged: the middle point stays at the line's middle.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": end, "to": [20, 0]}),
    );
    let s = sketch(&d);
    let geometry = &entity(&s, &line)["geometry"];
    let (a, b) = (xy(&geometry["start"]), xy(&geometry["end"]));
    let m = at(&s, &middle);
    assert!(
        near(m, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]),
        "{m:?} is not the middle of {a:?}-{b:?}"
    );
    assert!(near(b, [20.0, 0.0]), "{geometry}");
}

#[test]
fn a_midpoint_line_through_an_existing_point_and_its_refusals() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [5, 5]}),
    );
    let points = entities_of(&sketch(&d), "point").len();
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "mode": "midpoint",
               "center": "p1", "end": [8, 5]}),
    );
    let line = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    // p1 is the middle: only the line's two ends are new points.
    assert_eq!(entities_of(&s, "point").len(), points + 2, "{s}");
    let geometry = &entity(&s, &line)["geometry"];
    assert!(near(xy(&geometry["start"]), [2.0, 5.0]), "{geometry}");
    let midpoint = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "midpoint")
        .unwrap()
        .clone();
    assert_eq!(midpoint["point"], "p1", "{midpoint}");
    // Its middle and its end at one place; a mode it does not have; a
    // two-point line without a start.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "mode": "midpoint",
               "center": [1, 1], "end": [1, 1]}),
    );
    assert!(error.contains("apart"), "{error}");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "mode": "middle",
               "center": [1, 1], "end": [2, 1]}),
    );
    assert!(error.contains("two_point, midpoint"), "{error}");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "end": [2, 1]}),
    );
    assert!(error.contains("\"start\""), "{error}");
}
