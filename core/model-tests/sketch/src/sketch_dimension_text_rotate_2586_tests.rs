// SPDX-License-Identifier: MIT
//! Placed dimension values turn with geometry Move/Copy's Rotate turns
//! (mitcad#2586, follow-up of mitcad#2580): when `sketch.move` with
//! `rotate` turns all the geometry a dimension measures as one rigid
//! motion, its placed value turns about the same centre by the same angle,
//! so it keeps its place beside what it measures, also a value dragged by
//! hand; a dimension to geometry that stays keeps its value where it is.

use std::f64::consts::PI;

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

/// `p` turned counter-clockwise by `angle` about `center`.
fn turned(p: [f64; 2], center: [f64; 2], angle: f64) -> [f64; 2] {
    let (s, c) = angle.sin_cos();
    let (x, y) = (p[0] - center[0], p[1] - center[1]);
    [center[0] + c * x - s * y, center[1] + s * x + c * y]
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

/// The ticket's case: an unfixed 30 × 25 mm rectangle from (50, 40) to
/// (80, 65), its width dimensioned below it and its height to its right,
/// both values placed: the document, the rectangle's lines, its bottom
/// line and the two dimensions.
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

/// Checks that the rectangle and both placed values turned by `angle`
/// about `center` from `before`, and that the values still read 30 and
/// 25 mm.
fn assert_turned(case: &Case, before: &Value, center: [f64; 2], angle: f64) {
    let s = sketch(&case.d);
    for p in corners(before, case) {
        assert_near(at(&s, &p), turned(at(before, &p), center, angle), &p);
    }
    for k in [&case.width, &case.height] {
        assert_near(text(&s, k), turned(text(before, k), center, angle), k);
    }
    assert!((measured(&s, &case.width) - 30.0).abs() <= 1e-6, "{s}");
    assert!((measured(&s, &case.height) - 25.0).abs() <= 1e-6, "{s}");
}

#[test]
fn a_quarter_turn_turns_both_placed_values_and_undo_takes_them_back() {
    let mut case = rectangle_30_by_25();
    let before = sketch(&case.d);
    // Without a pivot Rotate turns about the rectangle's middle.
    let middle = [65.0, 52.5];
    command(
        &mut case.d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": case.lines,
               "rotate": {"angle": PI / 2.0}}),
    );
    assert_turned(&case, &before, middle, PI / 2.0);
    // The width's value, below the bottom line, is now to the right of
    // it; the height's, right of the right line, now above it.
    let s = sketch(&case.d);
    assert_near(text(&s, &case.width), [85.5, 52.5], "the width's value");
    assert_near(text(&s, &case.height), [65.0, 75.5], "the height's value");
    let moved = sketch(&case.d);

    // One undo step takes geometry and values back, redo brings both.
    command(&mut case.d, json!({"cmd": "undo"}));
    assert_turned(&case, &before, middle, 0.0);
    command(&mut case.d, json!({"cmd": "redo"}));
    assert_turned(&case, &before, middle, PI / 2.0);

    // Saved and opened again, the values are where the turn put them.
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
fn a_turn_about_a_pivot_carries_a_value_dragged_by_hand_with_its_place() {
    let mut case = rectangle_30_by_25();
    // The height's value dragged further out by hand first.
    command(
        &mut case.d,
        json!({"cmd": "sketch.set_dimension_text", "sketch": "F1",
               "dimension": case.height, "text": [95, 60]}),
    );
    let before = sketch(&case.d);
    assert_near(text(&before, &case.height), [95.0, 60.0], "dragged value");
    let center = [10.0, -5.0];
    command(
        &mut case.d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": case.lines,
               "rotate": {"center": center, "angle": -PI / 2.0}}),
    );
    assert_turned(&case, &before, center, -PI / 2.0);
    assert_near(
        text(&sketch(&case.d), &case.height),
        [75.0, -90.0],
        "turned value",
    );

    command(&mut case.d, json!({"cmd": "undo"}));
    assert_turned(&case, &before, center, 0.0);
}

#[test]
fn a_turn_off_the_axes_turns_the_values_too() {
    // By 30 degrees the height's vertical distance becomes an aligned one
    // (mitcad#1563); both values turn with the rectangle all the same.
    let mut case = rectangle_30_by_25();
    let before = sketch(&case.d);
    let center = [50.0, 40.0];
    command(
        &mut case.d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": case.lines,
               "rotate": {"center": center, "angle": PI / 6.0}}),
    );
    assert_turned(&case, &before, center, PI / 6.0);
}

#[test]
fn a_dimension_to_a_point_that_stays_keeps_its_value_in_place() {
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
    let middle = [65.0, 52.5];
    command(
        &mut case.d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": case.lines,
               "rotate": {"angle": PI / 2.0}}),
    );
    // The rectangle and its own values turned; the distance's value stays
    // where it was and it measures to the turned corner.
    assert_turned(&case, &before, middle, PI / 2.0);
    let s = sketch(&case.d);
    assert_near(at(&s, &origin), [5.0, 0.0], "the fixed point");
    assert_near(text(&s, &reach), [20.0, 30.0], "the distance's value");
    let now = turned([50.0, 40.0], middle, PI / 2.0);
    let expected = (now[0] - 5.0).hypot(now[1]);
    assert!((measured(&s, &reach) - expected).abs() <= 1e-6, "{s}");
}

#[test]
fn a_circles_value_turns_with_it_also_about_its_own_centre() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let made = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [15, 25], "radius": 5}),
    );
    let circle = made["made"][0].as_str().unwrap().to_owned();
    let diameter = first_id(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": 10,
                   "dimension": {"type": "diameter", "curve": circle}, "text": [25, 25]}),
        ),
        "dimensions",
        'k',
    );
    let s = sketch(&d);
    let centre = find(&s, "entities", &circle)["center"]
        .as_str()
        .unwrap()
        .to_owned();

    // About the sketch's origin: the centre goes round it, and the value
    // keeps its place beside the circle, turned with it.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [circle],
               "rotate": {"center": [0, 0], "angle": PI / 2.0}}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &centre), [-25.0, 15.0], "the centre");
    assert_near(text(&s, &diameter), [-25.0, 25.0], "the diameter's value");

    // About its own centre (no pivot): the centre stays, the value turns
    // round it.
    command(
        &mut d,
        json!({"cmd": "sketch.move", "sketch": "F1", "entities": [circle],
               "rotate": {"angle": PI / 2.0}}),
    );
    let s = sketch(&d);
    assert_near(at(&s, &centre), [-25.0, 15.0], "the centre");
    assert_near(text(&s, &diameter), [-35.0, 15.0], "the diameter's value");
    assert!((measured(&s, &diameter) - 10.0).abs() <= 1e-6, "{s}");
}
