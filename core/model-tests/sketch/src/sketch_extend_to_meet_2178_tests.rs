// SPDX-License-Identifier: MIT
//! Extend to Meet through the commands (mitcad#2178): `sketch.extend_to_meet`
//! brings two lines, or a line and an arc, to one shared corner point;
//! `result` steps through the other results, a pattern's curve is freed
//! first, curves that never meet are refused in words, and the work step
//! names both curves as changed.

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

fn id(v: &Value) -> String {
    v.as_str()
        .unwrap_or_else(|| panic!("no id: {v}"))
        .to_owned()
}

fn entity<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

/// A curve's solved ends.
fn ends(s: &Value, id: &str) -> ([f64; 2], [f64; 2]) {
    let g = &entity(s, id)["geometry"];
    (xy(&g["start"]), xy(&g["end"]))
}

/// A curve's end point ids.
fn end_ids(s: &Value, id: &str) -> (String, String) {
    let e = entity(s, id);
    (
        e["start"].as_str().unwrap().to_owned(),
        e["end"].as_str().unwrap().to_owned(),
    )
}

fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn line(d: &mut Document<MockKernel>, start: [f64; 2], end: [f64; 2]) -> String {
    id(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
    )["made"][0])
}

fn meet(
    d: &mut Document<MockKernel>,
    a: &str,
    at_a: [f64; 2],
    b: &str,
    at_b: [f64; 2],
    result: u32,
) -> Value {
    command(
        d,
        json!({"cmd": "sketch.extend_to_meet", "sketch": "F1", "a": a, "at_a": at_a,
               "b": b, "at_b": at_b, "result": result}),
    )
}

#[test]
fn two_lines_end_in_one_shared_point() {
    let mut d = with_sketch();
    let a = line(&mut d, [0.0, 0.0], [10.0, 0.0]);
    let b = line(&mut d, [20.0, 5.0], [20.0, 15.0]);
    let answer = meet(&mut d, &a, [2.0, 0.0], &b, [20.0, 12.0], 0);
    assert_eq!(answer["results"], 1, "{answer}");
    assert_eq!(answer["result"], 0, "{answer}");
    let joined = id(&answer["joined"]);
    let s = sketch(&d);
    assert_eq!(end_ids(&s, &a).1, joined);
    assert_eq!(end_ids(&s, &b).0, joined);
    let (p, q) = ends(&s, &a);
    assert!(near(p, [0.0, 0.0]) && near(q, [20.0, 0.0]), "{p:?} {q:?}");
    let (p, q) = ends(&s, &b);
    assert!(near(p, [20.0, 0.0]) && near(q, [20.0, 15.0]), "{p:?} {q:?}");
    // A work step of its own that names both curves as changed.
    let steps = s["steps"].as_array().unwrap();
    let last = steps.last().unwrap();
    assert_eq!(last["kind"], "extend_to_meet", "{last}");
    let changed: Vec<String> = last["changed"].as_array().unwrap().iter().map(id).collect();
    assert!(changed.contains(&a) && changed.contains(&b), "{last}");
}

#[test]
fn result_steps_to_the_other_crossing_of_a_line_and_an_arc() {
    let x = 75f64.sqrt();
    for (result, line_after, arc_end) in [
        (0, ([-2.0, 5.0], [x, 5.0]), [x, 5.0]),
        (1, ([-x, 5.0], [2.0, 5.0]), [-x, 5.0]),
        (2, ([-2.0, 5.0], [x, 5.0]), [x, 5.0]),
    ] {
        let mut d = with_sketch();
        let l = line(&mut d, [-2.0, 5.0], [2.0, 5.0]);
        let arc = id(&command(
            &mut d,
            json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
                   "start": [10, 0], "through": [0, 10], "end": [-10, 0]}),
        )["made"][0]);
        let pick = [
            10.0 * 20f64.to_radians().cos(),
            10.0 * 20f64.to_radians().sin(),
        ];
        let answer = meet(&mut d, &l, [1.5, 5.0], &arc, pick, result);
        assert_eq!(answer["results"], 2, "{answer}");
        assert_eq!(answer["result"], result % 2, "{answer}");
        let s = sketch(&d);
        let (p, q) = ends(&s, &l);
        assert!(
            near(p, line_after.0) && near(q, line_after.1),
            "{result}: {p:?} {q:?}"
        );
        // The arc keeps its start; its end reaches the crossing.
        let (p, q) = ends(&s, &arc);
        assert!(
            near(p, [10.0, 0.0]) && near(q, arc_end),
            "{result}: {p:?} {q:?}"
        );
        assert_eq!(end_ids(&s, &arc).1, id(&answer["joined"]));
    }
}

#[test]
fn a_patterns_curve_is_freed_first() {
    let mut d = with_sketch();
    let original = line(&mut d, [0.0, 0.0], [10.0, 0.0]);
    command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": [original],
               "direction": [1, 0], "count": [2, 1], "spacing": [20, 10]}),
    );
    let up = line(&mut d, [5.0, 5.0], [5.0, 15.0]);
    let answer = meet(&mut d, &original, [8.0, 0.0], &up, [5.0, 12.0], 0);
    let notes = answer["note_messages"].to_string();
    assert!(
        notes.contains("the pattern's copies are now plain sketch geometry"),
        "{answer}"
    );
    let s = sketch(&d);
    assert!(
        s["patterns"].as_array().is_none_or(Vec::is_empty),
        "{}",
        s["patterns"]
    );
    let (p, q) = ends(&s, &original);
    assert!(near(p, [5.0, 0.0]) && near(q, [10.0, 0.0]), "{p:?} {q:?}");
}

#[test]
fn parallel_lines_are_refused_in_words() {
    let mut d = with_sketch();
    let a = line(&mut d, [0.0, 0.0], [10.0, 0.0]);
    let b = line(&mut d, [0.0, 5.0], [10.0, 5.0]);
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.extend_to_meet", "sketch": "F1", "a": a, "at_a": [5, 0],
               "b": b, "at_b": [5, 5]}),
    );
    assert!(error.contains("parallel"), "{error}");
}
