// SPDX-License-Identifier: MIT
//! The `sketch_loop` query (mitcad#1541): the smallest closed loop a sketch
//! curve bounds, crossing a point where a third curve leaves; none for a
//! dangling curve or an open polyline; construction and normal curves not
//! mixed.

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
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

/// The loop's curves, sorted.
fn loop_of(doc: &Document<MockKernel>, curve: &str) -> Vec<String> {
    let answer = query(
        doc,
        json!({"query": "sketch_loop", "sketch": "F1", "curve": curve}),
    );
    let mut curves: Vec<String> = answer["curves"]
        .as_array()
        .unwrap_or_else(|| panic!("no curves in {answer}"))
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    curves.sort();
    curves
}

/// The point of the sketch at `at`.
fn point_at(doc: &Document<MockKernel>, at: [f64; 2]) -> String {
    let s = query(doc, json!({"query": "sketch", "uid": "F1"}));
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "point"
                && (e["at"][0].as_f64().unwrap() - at[0]).abs() < 1e-9
                && (e["at"][1].as_f64().unwrap() - at[1]).abs() < 1e-9
        })
        .unwrap_or_else(|| panic!("no point at {at:?} in {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn first_made(result: &Value) -> String {
    result["made"][0].as_str().unwrap().to_owned()
}

/// A 40 x 20 rectangle (c1..c4) and a line from its corner (0, 0) out.
fn rectangle_with_tail() -> (Document<MockKernel>, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 20}),
    );
    let corner = point_at(&d, [0.0, 0.0]);
    let tail = first_made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": corner, "end": [-20, -10]}),
    ));
    (d, tail)
}

#[test]
fn a_rectangle_side_gives_the_rectangle_past_the_corner_a_line_leaves() {
    let (d, tail) = rectangle_with_tail();
    let rectangle = vec!["c1", "c2", "c3", "c4"];
    for side in &rectangle {
        assert_eq!(loop_of(&d, side), rectangle, "side {side}");
    }
    assert!(loop_of(&d, &tail).is_empty(), "the tail bounds no loop");
}

#[test]
fn an_open_polyline_bounds_no_loop() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let a = first_made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    ));
    let corner = point_at(&d, [10.0, 0.0]);
    let b = first_made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": corner, "end": [10, 10]}),
    ));
    for curve in [a, b] {
        assert!(loop_of(&d, &curve).is_empty(), "{curve}");
    }
}

#[test]
fn construction_curves_make_loops_of_their_own_kind_only() {
    // A construction line closing nothing with normal curves: a
    // triangle of a construction line and two rectangle sides is no loop
    // of either kind.
    let (mut d, _) = rectangle_with_tail();
    let from = point_at(&d, [40.0, 0.0]);
    let to = point_at(&d, [0.0, 20.0]);
    let diagonal = first_made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": from, "end": to,
               "construction": true}),
    ));
    assert!(loop_of(&d, &diagonal).is_empty());
    assert_eq!(loop_of(&d, "c1"), vec!["c1", "c2", "c3", "c4"]);
    // A point is no curve: no loop.
    assert!(loop_of(&d, "p1").is_empty());
}
