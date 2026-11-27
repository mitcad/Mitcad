// SPDX-License-Identifier: MIT
//! Placed dimension values move with translated geometry (mitcad#2580,
//! GitHub mitcad/Mitcad#739): when a selection drag (`sketch.drag` with
//! `entities`) or Move/Copy's Translate (`sketch.move` with `by`) moves all
//! the geometry a dimension measures by one offset, its placed value moves
//! by the offset the geometry actually made; a dimension to geometry that
//! stays, or whose geometry changes shape, keeps its value where it is.

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

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn find(s: &Value, list: &str, id: &str) -> Value {
    s[list]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

/// Where a point is.
fn at(s: &Value, point: &str) -> [f64; 2] {
    xy(&find(s, "entities", point)["at"])
}

/// A line's start and end point ids.
fn ends(s: &Value, line: &str) -> [String; 2] {
    let e = find(s, "entities", line);
    [
        e["start"].as_str().unwrap().to_owned(),
        e["end"].as_str().unwrap().to_owned(),
    ]
}

/// Where a dimension's value is placed.
fn text(s: &Value, dimension: &str) -> [f64; 2] {
    xy(&find(s, "dimensions", dimension)["text"])
}

fn measured(s: &Value, dimension: &str) -> f64 {
    find(s, "dimensions", dimension)["measured"]
        .as_f64()
        .unwrap()
}

fn assert_near(a: [f64; 2], b: [f64; 2], what: &str) {
    assert!(
        (a[0] - b[0]).abs() <= 1e-6 && (a[1] - b[1]).abs() <= 1e-6,
        "{what}: {a:?}, expected {b:?}"
    );
}

fn plus(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn first_id(made: &Value, list: &str, prefix: char) -> String {
    made[list]
        .as_array()
        .unwrap_or_else(|| panic!("no {list} in {made}"))
        .iter()
        .filter_map(Value::as_str)
        .find(|id| id.starts_with(prefix))
        .unwrap_or_else(|| panic!("no {prefix} id in {made}"))
        .to_owned()
}

/// The ticket's case: an unfixed 30 × 25 mm rectangle away from the
/// sketch's origin, its width dimensioned below it and its height to its
/// right, both values placed: the document, the rectangle's lines, its
/// bottom line and the two dimensions.
struct Case {
    d: Document<MockKernel>,
    lines: Vec<String>,
    bottom: String,
    width: String,
    height: String,
}

fn rectangle_30_by_25() -> Case {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let made = command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point",
               "a": [50, 40], "b": [80, 65]}),
    );
    let lines: Vec<String> = made["made"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .filter(|id| id.starts_with('c'))
        .map(str::to_owned)
        .collect();
    assert_eq!(lines.len(), 4, "{made}");
    let s = sketch(&d);
    let line_where = |test: &dyn Fn([f64; 2], [f64; 2]) -> bool| {
        lines
            .iter()
            .find(|c| {
                let [a, b] = ends(&s, c);
                test(at(&s, &a), at(&s, &b))
            })
            .unwrap_or_else(|| panic!("no such line in {s}"))
            .clone()
    };
    let bottom = line_where(&|a, b| (a[1] - 40.0).abs() + (b[1] - 40.0).abs() <= 1e-9);
    let right = line_where(&|a, b| (a[0] - 80.0).abs() + (b[0] - 80.0).abs() <= 1e-9);
    // The width as a length of the bottom line, the height as a vertical
    // distance between the right line's ends (its points).
    let width = first_id(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": {"type": "length", "line": bottom}, "value": 30,
                   "text": [65, 32]}),
        ),
        "dimensions",
        'k',
    );
    let [low, high] = ends(&s, &right);
    let height = first_id(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": {"type": "vertical_distance", "a": low, "b": high},
                   "value": 25, "text": [88, 52.5]}),
        ),
        "dimensions",
        'k',
    );
    Case {
        d,
        lines,
        bottom,
        width,
        height,
    }
}

/// The corners of the rectangle, as point ids.
fn corners(s: &Value, case: &Case) -> Vec<String> {
    let mut points: Vec<String> = case.lines.iter().flat_map(|c| ends(s, c)).collect();
    points.sort();
    points.dedup();
    points
}

/// Checks that the rectangle and both placed values moved by `by` from
/// `before`, and that the values still read 30 and 25 mm.
fn assert_moved(case: &Case, before: &Value, by: [f64; 2]) {
    let s = sketch(&case.d);
    for p in corners(before, case) {
        assert_near(at(&s, &p), plus(at(before, &p), by), &p);
    }
    for k in [&case.width, &case.height] {
        assert_near(text(&s, k), plus(text(before, k), by), k);
    }
    assert!((measured(&s, &case.width) - 30.0).abs() <= 1e-6, "{s}");
    assert!((measured(&s, &case.height) - 25.0).abs() <= 1e-6, "{s}");
}

#[test]
fn a_selection_drag_carries_both_placed_values_and_undo_takes_them_back() {
    let mut case = rectangle_30_by_25();
    let before = sketch(&case.d);
    let by = [0.0, 20.0];
    command(
        &mut case.d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entities": case.lines, "by": by}),
    );
    assert_moved(&case, &before, by);
    let moved = sketch(&case.d);

    // One undo step takes geometry and values back, redo brings both.
    command(&mut case.d, json!({"cmd": "undo"}));
    assert_moved(&case, &before, [0.0, 0.0]);
    command(&mut case.d, json!({"cmd": "redo"}));
    assert_moved(&case, &before, by);

    // Saved and opened again, the values are where the drag put them.
    let mut reopened = Document::from_json(&case.d.to_json(), MockKernel::default()).unwrap();
    reopened.recompute();
    let r: Value = serde_json::from_str(
        &reopened
            .query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap();
    for k in [&case.width, &case.height] {
        assert_near(text(&r, k), text(&moved, k), k);
    }
}

#[test]
fn a_drag_in_steps_carries_the_values_by_every_step() {
    // A drag is many sketch.drag steps; each carries the values by its own
    // offset, so they end by the whole one.
    let mut case = rectangle_30_by_25();
    let before = sketch(&case.d);
    for _ in 0..4 {
        command(
            &mut case.d,
            json!({"cmd": "sketch.drag", "sketch": "F1", "entities": case.lines,
                   "by": [0.0, 5.0]}),
        );
    }
    assert_moved(&case, &before, [0.0, 20.0]);
}

#[test]
fn move_translate_carries_a_value_dragged_by_hand_with_its_offset() {
    let mut case = rectangle_30_by_25();
    // The height's value dragged further out by hand first.
    command(
        &mut case.d,
        json!({"cmd": "sketch.set_dimension_text", "sketch": "F1",
               "dimension": case.height, "text": [95, 60]}),
    );
    let before = sketch(&case.d);
    assert_near(text(&before, &case.height), [95.0, 60.0], "dragged value");
    let by = [-12.5, 20.0];
    command(
        &mut case.d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": case.lines, "by": by}),
    );
    assert_moved(&case, &before, by);
    assert_near(
        text(&sketch(&case.d), &case.height),
        [82.5, 80.0],
        "moved value",
    );

    command(&mut case.d, json!({"cmd": "undo"}));
    assert_moved(&case, &before, [0.0, 0.0]);
    command(&mut case.d, json!({"cmd": "redo"}));
    assert_moved(&case, &before, by);
}

#[test]
fn a_dimension_to_a_point_that_stays_keeps_its_value_and_measures_the_move() {
    let mut case = rectangle_30_by_25();
    // A driven distance from a fixed point to the rectangle's lower left
    // corner, its value placed.
    let origin = first_id(
        &command(
            &mut case.d,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [5, 0], "fixed": true}),
        ),
        "entities",
        'p',
    );
    let s = sketch(&case.d);
    let corner = ends(&s, &case.bottom)
        .into_iter()
        .min_by(|a, b| at(&s, a)[0].total_cmp(&at(&s, b)[0]))
        .unwrap();
    assert_near(at(&s, &corner), [50.0, 40.0], "lower left corner");
    let reach = first_id(
        &command(
            &mut case.d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1", "driven": true,
                   "dimension": {"type": "distance", "a": origin, "b": corner},
                   "text": [20, 30]}),
        ),
        "dimensions",
        'k',
    );
    let before = sketch(&case.d);
    let by = [0.0, 20.0];
    command(
        &mut case.d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": case.lines, "by": by}),
    );
    // The rectangle and its own values moved; the distance's value stays
    // and it measures from the point that stays to the moved corner.
    assert_moved(&case, &before, by);
    let s = sketch(&case.d);
    assert_near(at(&s, &origin), [5.0, 0.0], "the fixed point");
    assert_near(text(&s, &reach), [20.0, 30.0], "the distance's value");
    assert!((measured(&s, &reach) - 75.0).abs() <= 1e-6, "{s}");
}

#[test]
fn a_move_the_constraints_block_leaves_the_values_in_place() {
    let mut case = rectangle_30_by_25();
    // The lower left corner fixed: with the width and height the rectangle
    // cannot move.
    let s = sketch(&case.d);
    let corner = ends(&s, &case.bottom)
        .into_iter()
        .min_by(|a, b| at(&s, a)[0].total_cmp(&at(&s, b)[0]))
        .unwrap();
    command(
        &mut case.d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [corner]}),
    );
    let before = sketch(&case.d);
    // Refused or held, nothing moves, the values neither.
    let _ = case.d.command(
        &json!({"cmd": "sketch.drag", "sketch": "F1", "entities": case.lines, "by": [0, 20]})
            .to_string(),
    );
    assert_moved(&case, &before, [0.0, 0.0]);
}

#[test]
fn geometry_that_changes_shape_keeps_its_value_in_place() {
    // A free line's end dragged: the line stretches, its start stays, so
    // its length's value stays where it was put.
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let line = first_id(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [10, 10], "end": [40, 25]}),
        ),
        "entities",
        'c',
    );
    let length = first_id(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1", "driven": true,
                   "dimension": {"type": "length", "line": line}, "text": [25, 5]}),
        ),
        "dimensions",
        'k',
    );
    let s = sketch(&d);
    let [start, end] = ends(&s, &line);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": end, "to": [50, 20]}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &start), [10.0, 10.0], "the start");
    assert_near(at(&s, &end), [50.0, 20.0], "the end");
    assert_near(text(&s, &length), [25.0, 5.0], "the length's value");

    // The whole line dragged: both ends go by one offset, so the value
    // goes with them.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": line, "by": [5, -3]}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &start), [15.0, 7.0], "the start");
    assert_near(text(&s, &length), [30.0, 2.0], "the length's value");
}
