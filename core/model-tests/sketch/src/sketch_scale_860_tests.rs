// SPDX-License-Identifier: MIT
//! Scaling a sketch (mitcad#860): `sketch.scale`, and the first driving
//! dimension that scales a sketch drawn out of proportion to itself
//! (`sketch.add_dimension` with `scale_sketch`).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::kernel::Curve3;
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

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn find<'a>(list: &'a Value, id: &str) -> &'a Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {list}"))
}

/// Where a point is.
fn at(s: &Value, point: &str) -> [f64; 2] {
    let p = &find(&s["entities"], point)["at"];
    [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]
}

/// A line's length.
fn length(s: &Value, line: &str) -> f64 {
    let e = find(&s["entities"], line);
    let a = at(s, e["start"].as_str().unwrap());
    let b = at(s, e["end"].as_str().unwrap());
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// The baseline tolerance of geometric comparisons.
fn near(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= 1e-6, "{what}: {a}, expected {b}");
}

fn near2(a: [f64; 2], b: [f64; 2], what: &str) {
    assert!(
        (a[0] - b[0]).abs() <= 1e-6 && (a[1] - b[1]).abs() <= 1e-6,
        "{what}: {a:?}, expected {b:?}"
    );
}

fn ids(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect()
}

/// A new document with an empty Sketch1 (F1).
fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

/// A free rectangle from `a` to `b` (no dimensions) in `uid`: its lines,
/// bottom, right, top and left.
fn rectangle(d: &mut Document<MockKernel>, uid: &str, a: [f64; 2], b: [f64; 2]) -> Vec<String> {
    let made = command(
        d,
        json!({"cmd": "sketch.rectangle", "sketch": uid, "mode": "two_point", "a": a, "b": b}),
    );
    made["made"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect()
}

/// A driving length dimension on `line`, with the first-dimension rule.
fn first_dimension(d: &mut Document<MockKernel>, uid: &str, line: &str, value: f64) -> Value {
    command(
        d,
        json!({"cmd": "sketch.add_dimension", "sketch": uid,
               "dimension": {"type": "length", "line": line}, "value": value,
               "scale_sketch": true}),
    )
}

/// The rectangle 10 × 5 of the ticket, its corner at the origin, with a
/// circle and a text beside it.
fn drawn_10_by_5() -> (Document<MockKernel>, Vec<String>, String) {
    let mut d = with_sketch();
    let lines = rectangle(&mut d, "F1", [0.0, 0.0], [10.0, 5.0]);
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [20, 0],
               "radius": 2}),
    )["made"][0]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "A", "at": [0, -10],
               "height": 2}),
    );
    (d, lines, circle)
}

#[test]
fn the_first_dimension_scales_a_rectangle_drawn_10_by_5_to_100_by_50() {
    let (mut d, lines, circle) = drawn_10_by_5();
    let before = sketch(&d, "F1");
    near(length(&before, &lines[0]), 10.0, "drawn width");
    near(length(&before, &lines[1]), 5.0, "drawn height");
    let constraints = ids(&before["constraints"]);
    assert_eq!(constraints.len(), 4, "{}", before["constraints"]);

    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": lines[0]}, "value": 100,
               "text": [5, -3], "scale_sketch": true}),
    );
    near(added["scaled"].as_f64().unwrap(), 10.0, "factor");
    let k = added["dimensions"][0].as_str().unwrap().to_owned();

    let s = sketch(&d, "F1");
    // The whole sketch is ten times its drawn size, about the origin.
    near(length(&s, &lines[0]), 100.0, "width");
    near(length(&s, &lines[1]), 50.0, "height");
    near(length(&s, &lines[2]), 100.0, "top");
    near(length(&s, &lines[3]), 50.0, "left");
    let bottom = find(&s["entities"], &lines[0]);
    near2(
        at(&s, bottom["start"].as_str().unwrap()),
        [0.0, 0.0],
        "corner",
    );
    near2(
        at(&s, bottom["end"].as_str().unwrap()),
        [100.0, 0.0],
        "corner",
    );
    let c = find(&s["entities"], &circle);
    near(c["radius"].as_f64().unwrap(), 20.0, "circle radius");
    near2(
        at(&s, c["center"].as_str().unwrap()),
        [200.0, 0.0],
        "circle center",
    );
    let text = &s["texts"][0];
    near(text["height"].as_f64().unwrap(), 20.0, "text height");
    near2(
        [
            text["at"][0].as_f64().unwrap(),
            text["at"][1].as_f64().unwrap(),
        ],
        [0.0, -100.0],
        "text place",
    );
    // The constraints are kept, the dimension holds, and its value is
    // where it was put on the sketch as drawn.
    assert_eq!(ids(&s["constraints"]), constraints);
    let dim = find(&s["dimensions"], &k);
    near(dim["measured"].as_f64().unwrap(), 100.0, "dimension");
    near2(
        [
            dim["text"][0].as_f64().unwrap(),
            dim["text"][1].as_f64().unwrap(),
        ],
        [50.0, -30.0],
        "dimension text",
    );
}

#[test]
fn a_second_dimension_does_not_scale() {
    let (mut d, lines, circle) = drawn_10_by_5();
    first_dimension(&mut d, "F1", &lines[0], 100.0);
    let second = first_dimension(&mut d, "F1", &lines[1], 80.0);
    assert!(second.get("scaled").is_none(), "{second}");
    let s = sketch(&d, "F1");
    near(length(&s, &lines[0]), 100.0, "width");
    near(length(&s, &lines[1]), 80.0, "height");
    near(
        find(&s["entities"], &circle)["radius"].as_f64().unwrap(),
        20.0,
        "circle radius",
    );
}

#[test]
fn without_the_flag_driven_or_at_the_drawn_size_nothing_scales() {
    // Without `scale_sketch`: the dimension only constrains, as before.
    let mut d = with_sketch();
    let lines = rectangle(&mut d, "F1", [0.0, 0.0], [10.0, 5.0]);
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": lines[0]}, "value": 100}),
    );
    assert!(added.get("scaled").is_none(), "{added}");
    let s = sketch(&d, "F1");
    near(length(&s, &lines[0]), 100.0, "width");
    near(length(&s, &lines[1]), 5.0, "height");
    // A driven dimension measures and scales nothing.
    let mut d = with_sketch();
    let lines = rectangle(&mut d, "F1", [0.0, 0.0], [10.0, 5.0]);
    let driven = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": lines[0]}, "driven": true,
               "scale_sketch": true}),
    );
    assert!(driven.get("scaled").is_none(), "{driven}");
    near(length(&sketch(&d, "F1"), &lines[1]), 5.0, "height");
    // A value equal to the drawn size (a drawing tool's typed value).
    let mut d = with_sketch();
    let lines = rectangle(&mut d, "F1", [0.0, 0.0], [10.0, 5.0]);
    let same = first_dimension(&mut d, "F1", &lines[0], 10.0);
    assert!(same.get("scaled").is_none(), "{same}");
    near(length(&sketch(&d, "F1"), &lines[1]), 5.0, "height");
}

#[test]
fn a_sketch_with_a_projected_edge_does_not_scale() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let face = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": face}}),
    );
    // One edge of the face projected, linked.
    *d.kernel().curves.borrow_mut() = vec![Curve3::Line {
        start: [0.0, 0.0, 20.0],
        end: [60.0, 0.0, 20.0],
    }];
    let projected = command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "linked": true}),
    );
    let edge = projected["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| id.starts_with('c'))
        .unwrap()
        .to_owned();
    let lines = rectangle(&mut d, "F3", [100.0, 0.0], [110.0, 5.0]);
    let added = first_dimension(&mut d, "F3", &lines[0], 100.0);
    assert!(added.get("scaled").is_none(), "{added}");
    let s = sketch(&d, "F3");
    near(length(&s, &lines[0]), 100.0, "width");
    near(length(&s, &lines[1]), 5.0, "height as drawn");
    near(length(&s, &edge), 60.0, "the projected edge");
}

#[test]
fn a_fixed_point_stops_the_scale_and_the_origin_does_not() {
    // A fixed point of the user's: nothing scales.
    let mut d = with_sketch();
    let lines = rectangle(&mut d, "F1", [0.0, 0.0], [10.0, 5.0]);
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [30, 30], "fixed": true}),
    );
    let added = first_dimension(&mut d, "F1", &lines[0], 100.0);
    assert!(added.get("scaled").is_none(), "{added}");
    near(length(&sketch(&d, "F1"), &lines[1]), 5.0, "height");
    // The sketch's origin and axes: it scales about them.
    let mut d = with_sketch();
    let lines = rectangle(&mut d, "F1", [2.0, 1.0], [12.0, 6.0]);
    command(
        &mut d,
        json!({"cmd": "sketch.axis", "sketch": "F1", "axis": "x"}),
    );
    let added = first_dimension(&mut d, "F1", &lines[0], 100.0);
    near(added["scaled"].as_f64().unwrap(), 10.0, "factor");
    let s = sketch(&d, "F1");
    near(length(&s, &lines[1]), 50.0, "height");
    let bottom = find(&s["entities"], &lines[0]);
    near2(
        at(&s, bottom["start"].as_str().unwrap()),
        [20.0, 10.0],
        "corner, about the origin",
    );
}

#[test]
fn undo_restores_the_drawn_size_in_one_step() {
    let (mut d, lines, circle) = drawn_10_by_5();
    first_dimension(&mut d, "F1", &lines[0], 100.0);
    near(length(&sketch(&d, "F1"), &lines[1]), 50.0, "scaled height");
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d, "F1");
    near(length(&s, &lines[0]), 10.0, "drawn width");
    near(length(&s, &lines[1]), 5.0, "drawn height");
    near(
        find(&s["entities"], &circle)["radius"].as_f64().unwrap(),
        2.0,
        "drawn radius",
    );
    near(s["texts"][0]["height"].as_f64().unwrap(), 2.0, "drawn text");
    assert!(s["dimensions"].as_array().unwrap().is_empty(), "{s}");
    command(&mut d, json!({"cmd": "redo"}));
    let s = sketch(&d, "F1");
    near(length(&s, &lines[1]), 50.0, "scaled again");
    assert_eq!(s["dimensions"].as_array().unwrap().len(), 1);
}

#[test]
fn sketch_scale_scales_the_sketch_with_its_dimension_values() {
    let mut d = with_sketch();
    let lines = rectangle(&mut d, "F1", [0.0, 0.0], [10.0, 5.0]);
    for (line, value) in [(&lines[0], 10.0), (&lines[1], 5.0)] {
        command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": {"type": "length", "line": line}, "value": value}),
        );
    }
    command(
        &mut d,
        json!({"cmd": "sketch.scale", "sketch": "F1", "factor": 2.5, "center": [10, 0]}),
    );
    let s = sketch(&d, "F1");
    near(length(&s, &lines[0]), 25.0, "width");
    near(length(&s, &lines[1]), 12.5, "height");
    let bottom = find(&s["entities"], &lines[0]);
    near2(
        at(&s, bottom["start"].as_str().unwrap()),
        [-15.0, 0.0],
        "corner, about the center",
    );
    let measured: Vec<f64> = s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["measured"].as_f64().unwrap())
        .collect();
    assert_eq!(measured.len(), 2);
    near(measured[0], 25.0, "width dimension");
    near(measured[1], 12.5, "height dimension");
    // One undo step.
    command(&mut d, json!({"cmd": "undo"}));
    near(length(&sketch(&d, "F1"), &lines[0]), 10.0, "undone");

    // Refused: no factor, and a dimension that takes another parameter.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.scale", "sketch": "F1", "factor": 0}),
    );
    assert!(error.contains("greater than zero"), "{error}");
    let mut d2 = with_sketch();
    let lines2 = rectangle(&mut d2, "F1", [0.0, 0.0], [10.0, 5.0]);
    command(
        &mut d2,
        json!({"cmd": "add_parameter", "name": "width", "expression": "10 mm"}),
    );
    command(
        &mut d2,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": lines2[0]}, "value": "width"}),
    );
    let error = rejected(
        &mut d2,
        json!({"cmd": "sketch.scale", "sketch": "F1", "factor": 2}),
    );
    assert!(error.contains("width"), "{error}");
}
