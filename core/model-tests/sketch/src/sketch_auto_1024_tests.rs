// SPDX-License-Identifier: MIT
//! Constraints a drawing tool adds automatically (mitcad#1024):
//! `sketch.add_constraint` with `"auto": true` adds a constraint only when
//! the geometry already satisfies it, the others do not imply it and the
//! sketch still solves; otherwise it is refused with the reason and the
//! sketch stays as it was.

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

fn entity(s: &Value, id: &str) -> Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn field(s: &Value, id: &str, name: &str) -> String {
    entity(s, id)[name].as_str().unwrap().to_owned()
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    let e = entity(s, id);
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

/// Every point's position, by id.
fn positions(s: &Value) -> Vec<(String, [f64; 2])> {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "point")
        .map(|e| {
            let id = e["id"].as_str().unwrap().to_owned();
            (id.clone(), at(s, &id))
        })
        .collect()
}

/// The same points, none moved more than 1e-9 mm.
fn assert_unmoved(after: &[(String, [f64; 2])], before: &[(String, [f64; 2])]) {
    assert_eq!(after.len(), before.len(), "{after:?} / {before:?}");
    for ((ia, a), (ib, b)) in after.iter().zip(before) {
        assert_eq!(ia, ib);
        assert!(
            (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-9,
            "{ia} moved from {b:?} to {a:?}"
        );
    }
}

fn constraint_count(s: &Value) -> usize {
    s["constraints"].as_array().map_or(0, Vec::len)
}

/// The curve a drawing command made.
fn curve_made(result: &Value) -> String {
    result["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .find(|id| id.starts_with('c'))
        .unwrap_or_else(|| panic!("no curve in {result}"))
        .to_owned()
}

fn line(d: &mut Document<MockKernel>, start: [f64; 2], end: [f64; 2]) -> String {
    curve_made(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
    ))
}

fn auto(constraint: Value) -> Value {
    json!({"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": constraint,
           "auto": true})
}

fn new_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

#[test]
fn a_constraint_the_geometry_satisfies_is_added_without_moving_anything() {
    let mut d = new_sketch();
    let l = line(&mut d, [0.0, 5.0], [10.0, 5.0]);
    let before = positions(&sketch(&d));
    let added = command(&mut d, auto(json!({"type": "horizontal", "line": l})));
    let k = added["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert!(
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == k && c["type"] == "horizontal"),
        "{s}"
    );
    assert_unmoved(&positions(&s), &before);
}

#[test]
fn a_constraint_that_would_move_the_geometry_is_refused() {
    let mut d = new_sketch();
    // 0.01 mm off level: a horizontal would move an end.
    let l = line(&mut d, [0.0, 5.0], [10.0, 5.01]);
    let before = sketch(&d);
    let error = rejected(&mut d, auto(json!({"type": "horizontal", "line": l})));
    assert!(error.contains("auto constraint k"), "{error}");
    assert!(error.contains("moves p"), "{error}");
    let after = sketch(&d);
    assert_eq!(constraint_count(&after), constraint_count(&before));
    assert_unmoved(&positions(&after), &positions(&before));
    // Without `auto`, the constraint is added and the line levelled.
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": l}}),
    );
    assert_eq!(constraint_count(&sketch(&d)), constraint_count(&before) + 1);
}

/// A circle about the origin of radius 10 and a line from the left to its
/// top quadrant, ending on it: the quadrant's coincident and centre
/// alignment, the tangent, and then a horizontal the others imply.
#[test]
fn a_line_to_a_circle_s_top_quadrant_takes_its_hold_and_tangent_but_no_redundant_horizontal() {
    let mut d = new_sketch();
    let circle = curve_made(&command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [0.0, 0.0], "radius": 10.0}),
    ));
    let s = sketch(&d);
    let centre = field(&s, &circle, "center");
    let l = line(&mut d, [-20.0, 10.0], [0.0, 10.0]);
    let end = field(&sketch(&d), &l, "end");
    let before = positions(&sketch(&d));

    command(
        &mut d,
        auto(json!({"type": "coincident", "point": end, "entity": circle})),
    );
    command(
        &mut d,
        auto(json!({"type": "vertical_points", "a": end, "b": centre})),
    );
    command(
        &mut d,
        auto(json!({"type": "tangent", "a": circle, "b": l})),
    );
    let s = sketch(&d);
    assert_eq!(constraint_count(&s), 3, "{s}");
    assert_unmoved(&positions(&s), &before);

    // The point on the circle above its centre and the tangent make the
    // line level already.
    let error = rejected(&mut d, auto(json!({"type": "horizontal", "line": l})));
    assert!(error.contains("redundant"), "{error}");
    let after = sketch(&d);
    assert_eq!(
        constraint_count(&after),
        3,
        "nothing added or removed: {after}"
    );
    // The same tangent again is redundant too, and nothing is replaced
    // (a plain add_constraint may replace what a new one implies,
    // mitcad#287).
    let error = rejected(
        &mut d,
        auto(json!({"type": "tangent", "a": circle, "b": l})),
    );
    assert!(error.contains("redundant"), "{error}");
    assert_eq!(constraint_count(&sketch(&d)), 3);
}

#[test]
fn a_coincident_that_would_join_two_curve_ends_is_refused() {
    let mut d = new_sketch();
    let a = line(&mut d, [0.0, 0.0], [10.0, 0.0]);
    let b = line(&mut d, [10.0, 0.0], [10.0, 10.0]);
    let s = sketch(&d);
    let (end, start) = (field(&s, &a, "end"), field(&s, &b, "start"));
    if end == start {
        return; // joined when drawn: nothing to refuse
    }
    let error = rejected(
        &mut d,
        auto(json!({"type": "coincident", "point": end, "entity": start})),
    );
    assert!(error.contains("joins"), "{error}");
}
