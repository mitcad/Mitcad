// SPDX-License-Identifier: MIT
//! Tangent between a line and an arc whose start lies on the line
//! (mitcad#2006, GitHub mitcad/Mitcad#429), with the mock kernel: a
//! horizontal line and a 3-Point Arc whose start is held at the line's
//! midpoint take a Tangent, and the arc then leaves the line tangentially
//! at its start. Before, the tangency was the centre's distance from the
//! line, which the midpoint implies there, and the command refused it as
//! implied in full. The same with the arc starting at the line's end and
//! with its start held on the line by a coincident.

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

fn constrain(d: &mut Document<MockKernel>, constraint: Value) -> Value {
    command(
        d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": constraint}),
    )
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

fn id(v: &Value) -> String {
    v.as_str().unwrap().to_owned()
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    xy(&entity(s, id)["at"])
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn length(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

/// A horizontal line from (-20, 10) to (20, 10): its id and end points.
fn level_line(d: &mut Document<MockKernel>) -> (String, String, String) {
    let made = command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [-20, 10], "end": [20, 10]}),
    );
    let line = made["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(id)
        .find(|e| e.starts_with('c'))
        .unwrap_or_else(|| panic!("no line in {made}"));
    constrain(d, json!({"type": "horizontal", "line": line}));
    let l = entity(&sketch(d), &line);
    (line, id(&l["start"]), id(&l["end"]))
}

/// A 3-Point Arc from `start` (a point input at `from`) through `from` +
/// (4, 5) to `from` + (9, 7): its id and the end point at `from` (the
/// arc keeps its ends counter-clockwise, so that may be its end).
fn arc(d: &mut Document<MockKernel>, start: Value, from: [f64; 2]) -> (String, String) {
    let made = command(
        d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": start, "through": [from[0] + 4.0, from[1] + 5.0],
               "end": [from[0] + 9.0, from[1] + 7.0]}),
    );
    let arc = id(&made["made"][0]);
    let s = sketch(d);
    let e = entity(&s, &arc);
    let start = [id(&e["start"]), id(&e["end"])]
        .into_iter()
        .min_by(|a, b| {
            let (da, db) = (length(sub(at(&s, a), from)), length(sub(at(&s, b), from)));
            da.total_cmp(&db)
        })
        .unwrap();
    (arc, start)
}

/// After Tangent between `line` and `arc`: the sketch solves with nothing
/// redundant, the line is level, `start` lies on it and the arc's radius
/// there is square to it (the arc leaves the line tangentially at its
/// start).
fn assert_tangent_at_start(d: &Document<MockKernel>, line: &str, arc: &str, start: &str) {
    let s = sketch(d);
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    let l = entity(&s, line);
    let (a, b) = (at(&s, &id(&l["start"])), at(&s, &id(&l["end"])));
    assert!((a[1] - b[1]).abs() <= 1e-6, "the line is not level: {s}");
    let p = at(&s, start);
    assert!(
        (p[1] - a[1]).abs() <= 1e-6,
        "the start is off the line: {s}"
    );
    let c = at(&s, &id(&entity(&s, arc)["center"]));
    let (radius, along) = (sub(p, c), sub(b, a));
    assert!(
        dot(radius, along).abs() <= 1e-6 * length(radius) * length(along),
        "the radius {radius:?} at the start is not square to the line: {s}"
    );
}

#[test]
fn an_arc_starting_at_the_line_s_midpoint_takes_a_tangent() {
    let mut d = with_sketch();
    let (line, _, _) = level_line(&mut d);
    let (arc, start) = arc(&mut d, json!([0, 10]), [0.0, 10.0]);
    constrain(
        &mut d,
        json!({"type": "midpoint", "point": start, "curve": line}),
    );
    let added = constrain(&mut d, json!({"type": "tangent", "a": line, "b": arc}));
    assert!(
        added["constraints"]
            .as_array()
            .is_some_and(|k| !k.is_empty()),
        "{added}"
    );
    assert_tangent_at_start(&d, &line, &arc, &start);
    // Still at the line's midpoint.
    let s = sketch(&d);
    let l = entity(&s, &line);
    let (a, b) = (at(&s, &id(&l["start"])), at(&s, &id(&l["end"])));
    let p = at(&s, &start);
    assert!((p[0] - (a[0] + b[0]) / 2.0).abs() <= 1e-6, "{s}");
}

#[test]
fn an_arc_starting_at_the_line_s_end_takes_a_tangent() {
    let mut d = with_sketch();
    let (line, _, end) = level_line(&mut d);
    let (arc, start) = arc(&mut d, json!(end.clone()), [20.0, 10.0]);
    assert_eq!(start, end);
    constrain(&mut d, json!({"type": "tangent", "a": line, "b": arc}));
    assert_tangent_at_start(&d, &line, &arc, &start);
}

#[test]
fn an_arc_starting_on_the_line_by_a_coincident_takes_a_tangent() {
    let mut d = with_sketch();
    let (line, _, _) = level_line(&mut d);
    let (arc, start) = arc(&mut d, json!([5, 10]), [5.0, 10.0]);
    constrain(
        &mut d,
        json!({"type": "coincident", "point": start, "entity": line}),
    );
    constrain(&mut d, json!({"type": "tangent", "a": line, "b": arc}));
    assert_tangent_at_start(&d, &line, &arc, &start);
}
