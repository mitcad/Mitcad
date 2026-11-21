// SPDX-License-Identifier: MIT
//! Sketch Scale (mitcad#2109): `sketch.scale` with `entities` scales the
//! selected points, curves and texts about a point, their dimensions
//! scaled with them or kept; what ties them to the rest stays while it
//! holds and is removed otherwise; Create Copy scales copies. The whole
//! sketch without `entities`, as before.

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

/// A line's start point.
fn start(s: &Value, line: &str) -> String {
    find(&s["entities"], line)["start"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A line's end point.
fn end(s: &Value, line: &str) -> String {
    find(&s["entities"], line)["end"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A line's length.
fn length(s: &Value, line: &str) -> f64 {
    let a = at(s, &start(s, line));
    let b = at(s, &end(s, line));
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// What a dimension measures.
fn measured(s: &Value, dimension: &str) -> f64 {
    find(&s["dimensions"], dimension)["measured"]
        .as_f64()
        .unwrap()
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

fn strings(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect()
}

/// A free rectangle from `a` to `b`: its lines, bottom, right, top and
/// left.
fn rectangle(d: &mut Document<MockKernel>, a: [f64; 2], b: [f64; 2]) -> Vec<String> {
    let made = command(
        d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point", "a": a, "b": b}),
    );
    strings(&made["made"])
}

/// A driving length dimension on `line` with `value`: its id.
fn dimension(d: &mut Document<MockKernel>, line: &str, value: Value) -> String {
    let added = command(
        d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": line}, "value": value}),
    );
    added["dimensions"][0].as_str().unwrap().to_owned()
}

/// Sketch1 with two rectangles: A from (10, 10) to (20, 15), B from
/// (40, 10) to (50, 15), each held horizontal and vertical. A's width and
/// (with `height`) its height are dimensioned, B's width.
struct Two {
    d: Document<MockKernel>,
    a: Vec<String>,
    b: Vec<String>,
    width: String,
    height: Option<String>,
    b_width: String,
}

fn two_rectangles(height: bool) -> Two {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let a = rectangle(&mut d, [10.0, 10.0], [20.0, 15.0]);
    let b = rectangle(&mut d, [40.0, 10.0], [50.0, 15.0]);
    let width = dimension(&mut d, &a[0], json!(10));
    let height = height.then(|| dimension(&mut d, &a[1], json!(5)));
    let b_width = dimension(&mut d, &b[0], json!(10));
    Two {
        d,
        a,
        b,
        width,
        height,
        b_width,
    }
}

/// `sketch.scale` of rectangle A's lines.
fn scale_a(two: &Two, factor: f64, center: &str, dimensions: &str) -> Value {
    json!({"cmd": "sketch.scale", "sketch": "F1", "entities": two.a, "factor": factor,
           "center": center, "dimensions": dimensions})
}

/// B as it was drawn: its corners and its dimension.
fn b_untouched(s: &Value, two: &Two) {
    near2(at(s, &start(s, &two.b[0])), [40.0, 10.0], "B's corner");
    near2(at(s, &end(s, &two.b[1])), [50.0, 15.0], "B's far corner");
    near(measured(s, &two.b_width), 10.0, "B's width dimension");
}

#[test]
fn a_selected_rectangle_scales_about_its_corner_with_its_dimensions() {
    let mut two = two_rectangles(true);
    let s = sketch(&two.d);
    let corner = start(&s, &two.a[0]);
    near2(at(&s, &corner), [10.0, 10.0], "A's corner");
    let constraints = ids(&s["constraints"]);

    let cmd = scale_a(&two, 2.0, &corner, "scale");
    command(&mut two.d, cmd);
    let s = sketch(&two.d);
    near(length(&s, &two.a[0]), 20.0, "A's width");
    near(length(&s, &two.a[1]), 10.0, "A's height");
    near2(at(&s, &corner), [10.0, 10.0], "the corner stays");
    near2(at(&s, &end(&s, &two.a[1])), [30.0, 20.0], "A's far corner");
    near(
        measured(&s, &two.width),
        20.0,
        "A's width dimension doubled",
    );
    near(
        measured(&s, two.height.as_ref().unwrap()),
        10.0,
        "A's height dimension doubled",
    );
    b_untouched(&s, &two);
    assert_eq!(ids(&s["constraints"]), constraints, "all constraints kept");

    // One undo step.
    command(&mut two.d, json!({"cmd": "undo"}));
    let s = sketch(&two.d);
    near(length(&s, &two.a[0]), 10.0, "undone");
    near(measured(&s, &two.width), 10.0, "undone dimension");
}

#[test]
fn kept_dimensions_refuse_a_rectangle_they_fix_and_solve_one_they_do_not() {
    // Width and height kept: they fix A's size, so it cannot be scaled.
    let mut two = two_rectangles(true);
    let corner = start(&sketch(&two.d), &two.a[0]);
    let cmd = scale_a(&two, 2.0, &corner, "keep");
    let error = rejected(&mut two.d, cmd);
    assert!(error.contains("fix the size of the selection"), "{error}");
    assert!(error.contains("[conflicts: "), "{error}");
    near(length(&sketch(&two.d), &two.a[1]), 5.0, "nothing changed");

    // The width alone kept: it stays 10 mm, the height doubles.
    let mut two = two_rectangles(false);
    let corner = start(&sketch(&two.d), &two.a[0]);
    let cmd = scale_a(&two, 2.0, &corner, "keep");
    command(&mut two.d, cmd);
    let s = sketch(&two.d);
    near(length(&s, &two.a[0]), 10.0, "A's width kept");
    near(measured(&s, &two.width), 10.0, "A's width dimension kept");
    near(length(&s, &two.a[1]), 10.0, "A's height doubled");
    b_untouched(&s, &two);
}

#[test]
fn a_constraint_to_the_rest_stays_while_it_holds_and_goes_with_a_note() {
    let mut two = two_rectangles(true);
    let s = sketch(&two.d);
    let corner = start(&s, &two.a[0]);
    let far = end(&s, &two.a[1]);
    near2(at(&s, &far), [20.0, 15.0], "A's far corner");
    // Q below the corner, R level with the far corner, both outside A.
    let q = strings(
        &command(
            &mut two.d,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [10, -20]}),
        )["entities"],
    )[0]
    .clone();
    let r = strings(
        &command(
            &mut two.d,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [60, 15]}),
        )["entities"],
    )[0]
    .clone();
    let plumb = command(
        &mut two.d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "vertical_points", "a": corner, "b": q}}),
    )["constraints"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let level = command(
        &mut two.d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal_points", "a": far, "b": r}}),
    )["constraints"][0]
        .as_str()
        .unwrap()
        .to_owned();

    let cmd = scale_a(&two, 2.0, &corner, "scale");
    let result = command(&mut two.d, cmd);
    // The person reads the whole message, the count its only value; the
    // agents' note names the constraint.
    let shown = result["note_messages"].to_string();
    assert!(
        shown.contains("Constraints that tied the scaled geometry to the rest were removed: 1"),
        "{result}"
    );
    let notes = result["notes"].to_string();
    assert!(
        notes.contains(&format!("{level} tied the scaled geometry to the rest")),
        "{result}"
    );
    let s = sketch(&two.d);
    let constraints = ids(&s["constraints"]);
    assert!(
        constraints.contains(&plumb),
        "the corner stays plumb with Q"
    );
    assert!(
        !constraints.contains(&level),
        "the far corner left R's level"
    );
    near2(at(&s, &q), [10.0, -20.0], "Q stays");
    near2(at(&s, &r), [60.0, 15.0], "R stays");
    near2(at(&s, &far), [30.0, 20.0], "A's far corner scaled");
    b_untouched(&s, &two);
}

#[test]
fn a_parameter_valued_dimension_refuses_scale_and_is_kept_with_keep() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "width", "expression": "10 mm"}),
    );
    let a = rectangle(&mut d, [10.0, 10.0], [20.0, 15.0]);
    let width = dimension(&mut d, &a[0], json!("width"));
    let corner = start(&sketch(&d), &a[0]);
    let scale = |dimensions: &str| {
        json!({"cmd": "sketch.scale", "sketch": "F1", "entities": a, "factor": 2,
               "center": corner, "dimensions": dimensions})
    };

    let error = rejected(&mut d, scale("scale"));
    assert!(
        error.contains("a dimension of the selection takes its value from a parameter"),
        "{error}"
    );
    assert!(error.contains(&format!("[conflicts: {width}]")), "{error}");

    command(&mut d, scale("keep"));
    let s = sketch(&d);
    near(length(&s, &a[0]), 10.0, "the parameter's width");
    near(length(&s, &a[1]), 10.0, "the height doubled");
    let parameters: Value = serde_json::from_str(
        &d.query(&json!({"query": "parameters"}).to_string())
            .unwrap(),
    )
    .unwrap();
    let value = find_named(&parameters, "width")["value"].as_f64().unwrap();
    near(value, 10.0, "the parameter unchanged");
}

fn find_named<'a>(list: &'a Value, name: &str) -> &'a Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no {name} in {list}"))
}

#[test]
fn create_copy_scales_copies_and_leaves_the_selection() {
    let mut two = two_rectangles(true);
    let s = sketch(&two.d);
    let corner = start(&s, &two.a[0]);
    let constraints = s["constraints"].as_array().unwrap().len();
    let mut cmd = scale_a(&two, 2.0, &corner, "scale");
    cmd["copy"] = json!(true);
    let made = strings(&command(&mut two.d, cmd)["made"]);
    assert_eq!(made.len(), 4, "{made:?}");
    let s = sketch(&two.d);
    near(length(&s, &two.a[0]), 10.0, "A stays");
    near(length(&s, &made[0]), 20.0, "the copy's width");
    near(length(&s, &made[1]), 10.0, "the copy's height");
    near2(
        at(&s, &start(&s, &made[0])),
        [10.0, 10.0],
        "the copy's corner",
    );
    near2(
        at(&s, &end(&s, &made[1])),
        [30.0, 20.0],
        "the copy's far corner",
    );
    // The copies are held horizontal and vertical as A is.
    assert_eq!(
        s["constraints"].as_array().unwrap().len(),
        constraints + 4,
        "{}",
        s["constraints"]
    );
    b_untouched(&s, &two);
}

#[test]
fn a_circle_and_a_text_scale_about_the_circle_center() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let circle = strings(
        &command(
            &mut d,
            json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
                   "center": [10, 0], "radius": 5}),
        )["made"],
    )[0]
    .clone();
    let text = strings(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_text", "sketch": "F1", "text": "A", "at": [10, 10],
                   "height": 2}),
        )["texts"],
    )[0]
    .clone();
    let s = sketch(&d);
    let center = find(&s["entities"], &circle)["center"]
        .as_str()
        .unwrap()
        .to_owned();
    let diameter = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "diameter", "curve": circle}, "value": 10}),
    )["dimensions"][0]
        .as_str()
        .unwrap()
        .to_owned();

    command(
        &mut d,
        json!({"cmd": "sketch.scale", "sketch": "F1", "entities": [circle, text],
               "factor": 1.5, "center": center}),
    );
    let s = sketch(&d);
    near(
        find(&s["entities"], &circle)["radius"].as_f64().unwrap(),
        7.5,
        "radius",
    );
    near(measured(&s, &diameter), 15.0, "diameter dimension");
    near2(at(&s, &center), [10.0, 0.0], "the center stays");
    let t = find(&s["texts"], &text);
    near(t["height"].as_f64().unwrap(), 3.0, "text height");
    near2(
        [t["at"][0].as_f64().unwrap(), t["at"][1].as_f64().unwrap()],
        [10.0, 15.0],
        "text place",
    );
}

#[test]
fn the_whole_sketch_without_entities_as_before() {
    let mut two = two_rectangles(true);
    command(
        &mut two.d,
        json!({"cmd": "sketch.scale", "sketch": "F1", "factor": 2}),
    );
    let s = sketch(&two.d);
    near2(at(&s, &start(&s, &two.b[0])), [80.0, 20.0], "B scaled too");
    near(measured(&s, &two.b_width), 20.0, "B's width dimension");
    near(measured(&s, &two.width), 20.0, "A's width dimension");

    // Nothing but the origin: refused in words.
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.scale", "sketch": "F1", "entities": [], "factor": 2}),
    );
    assert!(error.contains("\"entities\" is empty"), "{error}");
}
