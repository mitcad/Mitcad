// SPDX-License-Identifier: MIT
//! Sketch patterns whose spacing or angle uses a driven dimension of their
//! own sketch (mitcad#2057): accepted and solved after the measurement,
//! like a driving dimension that uses one (mitcad#1116), whether or not the
//! originals are fully constrained; refused as a circular dependency, with
//! the pattern named, when its copies can move what the driven dimension
//! measures. The values that still may not use the sketch's references (an
//! offset's distance) are named in the refusal.

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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn parameters(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "parameters"}))
}

fn parameter(doc: &Document<MockKernel>, name: &str) -> Value {
    parameters(doc)
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

fn value(doc: &Document<MockKernel>, name: &str) -> f64 {
    parameter(doc, name)["value"].as_f64().unwrap()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "sketch", "uid": "F1"}))
}

fn status(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "feature", "uid": "F1"}))
}

fn entity(s: &Value, id: &str) -> Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no entity {id} in {s}"))
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    let e = entity(s, id);
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

/// The copy of `original` in the first copy of the sketch's only (or
/// first) pattern.
fn copy_of(s: &Value, original: &str) -> String {
    s["patterns"][0]["copies"][0]["entities"][original]
        .as_str()
        .unwrap_or_else(|| panic!("no copy of {original}: {}", s["patterns"]))
        .to_owned()
}

/// Where the first copy of `original` lies from it.
fn shift(doc: &Document<MockKernel>, original: &str) -> [f64; 2] {
    let s = sketch(doc);
    let a = at(&s, original);
    let b = at(&s, &copy_of(&s, original));
    [b[0] - a[0], b[1] - a[1]]
}

fn set_dimension(dimension: &str, value: &str) -> Value {
    json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension, "value": value})
}

fn made(result: &Value) -> String {
    result["made"][0]
        .as_str()
        .unwrap_or_else(|| panic!("nothing made: {result}"))
        .to_owned()
}

fn driven(doc: &mut Document<MockKernel>, dimension: Value) -> (String, String) {
    let added = command(
        doc,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "dimension": dimension,
               "driven": true}),
    );
    (
        added["dimensions"][0].as_str().unwrap().to_owned(),
        added["parameters"][0].as_str().unwrap().to_owned(),
    )
}

/// Sketch1 (F1): a 30 x 100 rectangle with its first corner p5 fixed at
/// the origin (lines c1-c4 from p5 counter-clockwise, width k5 = d1 on the
/// bottom c1, height k6 = d2 on the right side c2), its diagonal p5-p7 a
/// construction line c9 whose driven length k7 is d3. `anchored` false
/// frees the first corner: the rectangle can then move as a whole, so its
/// entities are not fully constrained.
fn measured_rectangle(anchored: bool) -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 30, "height": 100}),
    );
    if !anchored {
        command(
            &mut d,
            json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": ["p5"],
                   "fixed": false}),
        );
    }
    let diagonal = made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p5", "end": "p7",
               "construction": true}),
    ));
    assert_eq!(diagonal, "c9");
    let (k, name) = driven(&mut d, json!({"type": "length", "line": "c9"}));
    assert_eq!((k.as_str(), name.as_str()), ("k7", "d3"));
    d
}

/// The rectangle's lines patterned once along x, `spacing` apart: the
/// pattern k8.
fn pattern_along_x(d: &mut Document<MockKernel>, spacing: Value) -> String {
    let made = command(
        d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1",
               "entities": ["c1", "c2", "c3", "c4"], "direction": [1, 0], "count": [2, 1],
               "spacing": [spacing, 10]}),
    );
    made["constraints"][0].as_str().unwrap().to_owned()
}

fn diagonal(width: f64, height: f64) -> f64 {
    width.hypot(height)
}

/// The spacing set to the driven diagonal of fully constrained originals:
/// accepted, the copies that far; a new width moves them by the new
/// diagonal in the same recompute; undo, redo and a saved and reopened
/// design keep it.
#[test]
fn a_spacing_uses_the_driven_diagonal_of_fully_constrained_originals() {
    let mut d = measured_rectangle(true);
    let pattern = pattern_along_x(&mut d, json!("d3"));
    assert_eq!(pattern, "k8");
    assert_eq!(status(&d)["status"], "ok", "{}", status(&d));
    assert!((value(&d, "d3") - diagonal(30.0, 100.0)).abs() < 1e-9);
    assert!(near(shift(&d, "p5"), [diagonal(30.0, 100.0), 0.0]));
    assert!(near(shift(&d, "p7"), [diagonal(30.0, 100.0), 0.0]));
    // The measured dimension stays driven and its reference read-only.
    assert_eq!(parameter(&d, "d3")["kind"], "reference");
    command(&mut d, set_dimension("k5", "40"));
    assert_eq!(status(&d)["status"], "ok", "{}", status(&d));
    assert!((value(&d, "d3") - diagonal(40.0, 100.0)).abs() < 1e-9);
    assert!(near(shift(&d, "p5"), [diagonal(40.0, 100.0), 0.0]));
    d.undo().unwrap();
    assert!(near(shift(&d, "p5"), [diagonal(30.0, 100.0), 0.0]));
    d.redo().unwrap();
    assert!(near(shift(&d, "p5"), [diagonal(40.0, 100.0), 0.0]));
    // Saved and opened again: the pattern still uses the reference.
    let saved = d.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(status(&loaded)["status"], "ok", "{}", status(&loaded));
    assert_eq!(parameter(&loaded, "d3")["kind"], "reference");
    assert!(near(shift(&loaded, "p5"), [diagonal(40.0, 100.0), 0.0]));
    command(&mut loaded, set_dimension("k6", "80"));
    assert!((value(&loaded, "d3") - diagonal(40.0, 80.0)).abs() < 1e-9);
    assert!(near(shift(&loaded, "p5"), [diagonal(40.0, 80.0), 0.0]));
}

/// The originals free to move (not fully constrained): the pattern moves
/// only its copies, so its own binding makes no cycle with the diagonal
/// of the originals. Its own spacing parameter given the reference as an
/// expression, as the parameter table does, and Edit Pattern naming it,
/// as the Pattern panel does, are both accepted.
#[test]
fn a_spacing_uses_the_driven_diagonal_of_originals_that_are_not_fully_constrained() {
    let mut d = measured_rectangle(false);
    let p5 = entity(&sketch(&d), "p5");
    assert_eq!(p5["fully_constrained"], false, "{p5}");
    let pattern = pattern_along_x(&mut d, json!(50));
    let own = sketch(&d)["patterns"][0]["spacing"][0]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(near(shift(&d, "p5"), [50.0, 0.0]));
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": own, "expression": "d3"}),
    );
    assert_eq!(status(&d)["status"], "ok", "{}", status(&d));
    assert!(near(shift(&d, "p5"), [diagonal(30.0, 100.0), 0.0]));
    command(&mut d, set_dimension("k5", "40"));
    assert!(near(shift(&d, "p5"), [diagonal(40.0, 100.0), 0.0]));
    // Back to a number, then the reference itself in the slot.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "spacing": [50, 10]}),
    );
    assert!(near(shift(&d, "p5"), [50.0, 0.0]));
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "spacing": ["d3", 10]}),
    );
    assert_eq!(status(&d)["status"], "ok", "{}", status(&d));
    assert!(near(shift(&d, "p5"), [diagonal(40.0, 100.0), 0.0]));
    command(&mut d, set_dimension("k6", "80"));
    assert!(near(shift(&d, "p5"), [diagonal(40.0, 80.0), 0.0]));
}

/// A circular pattern's angle from a driven angle of its sketch: the
/// copies turn by the measured angle, and by the new one when the
/// originals change.
#[test]
fn a_circular_angle_uses_a_driven_angle() {
    let mut d = measured_rectangle(true);
    let (_, angle) = driven(&mut d, json!({"type": "angle", "a": "c1", "b": "c9"}));
    assert_eq!(angle, "d4");
    command(
        &mut d,
        json!({"cmd": "sketch.circular_pattern", "sketch": "F1", "entities": ["c2"],
               "center": "p5", "count": 2, "angle": "d4"}),
    );
    assert_eq!(status(&d)["status"], "ok", "{}", status(&d));
    // The copy of p6 (30, 0) turned about p5 at the origin.
    let turned = |d: &Document<MockKernel>, width: f64| {
        let theta = value(d, "d4");
        let s = sketch(d);
        (
            at(&s, &copy_of(&s, "p6")),
            [width * theta.cos(), width * theta.sin()],
        )
    };
    let measured = value(&d, "d4");
    let expected = 100f64.atan2(30.0);
    assert!(
        (measured - expected).abs() < 1e-9
            || (measured - (std::f64::consts::PI - expected)).abs() < 1e-9,
        "{measured}"
    );
    let (got, wanted) = turned(&d, 30.0);
    assert!(near(got, wanted), "{got:?} {wanted:?}");
    command(&mut d, set_dimension("k5", "40"));
    assert!((value(&d, "d4") - measured).abs() > 1e-3);
    let (got, wanted) = turned(&d, 40.0);
    assert!(near(got, wanted), "{got:?} {wanted:?}");
}

/// A copy tied to the measured geometry: the pattern could move what the
/// spacing uses, so the command that ties them is refused, naming the
/// pattern and the driven dimension, and nothing changes.
#[test]
fn a_copy_constrained_to_the_measured_geometry_is_a_cycle() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 30, "height": 100}),
    );
    // A free line beside it, its length measured (k7, d3).
    let line = made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [200, 0], "end": [200, 50]}),
    ));
    let (k, reference) = driven(&mut d, json!({"type": "length", "line": line}));
    assert_eq!((k.as_str(), reference.as_str()), ("k7", "d3"));
    let pattern = pattern_along_x(&mut d, json!("d3"));
    assert!(near(shift(&d, "p6"), [50.0, 0.0]));
    let s = sketch(&d);
    let copy = copy_of(&s, "p6");
    let start = entity(&s, &line)["start"].as_str().unwrap().to_owned();
    let before = parameters(&d);
    let depth = d.undo_depth();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "distance", "a": copy, "b": start}}),
    );
    assert!(
        error.contains(&format!("the pattern {pattern} ("))
            && error.contains("uses d3")
            && error.contains("driven dimension k7 (d3)")
            && error.contains("would depend on itself")
            && error.contains("not the copies"),
        "{error}"
    );
    assert_eq!(parameters(&d), before);
    assert_eq!(d.undo_depth(), depth);
}

/// A driven dimension that measures a copy of the pattern: the pattern
/// cannot use it, as a command or in a design opened with it.
#[test]
fn a_driven_dimension_measuring_a_copy_is_a_cycle() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 30, "height": 100}),
    );
    let pattern = pattern_along_x(&mut d, json!(150));
    let s = sketch(&d);
    let own = s["patterns"][0]["spacing"][0].as_str().unwrap().to_owned();
    let copy = copy_of(&s, "c1");
    let (k, reference) = driven(&mut d, json!({"type": "length", "line": copy}));
    let set =
        json!({"cmd": "set_parameter", "name": own, "expression": format!("{reference} * 5")});
    let error = rejected(&mut d, set);
    assert!(
        error.contains(&format!("the pattern {pattern} ({own}"))
            && error.contains(&format!("uses {reference}"))
            && error.contains(&format!("driven dimension {k} ({reference})"))
            && error.contains("would depend on itself"),
        "{error}"
    );
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "spacing": [reference, 10]}),
    );
    assert!(error.contains("would depend on itself"), "{error}");
    assert!(near(shift(&d, "p5"), [150.0, 0.0]));
    // A file written elsewhere with the cycle: the recompute names it.
    let mut file: Value = serde_json::from_str(&d.to_json()).unwrap();
    let parameter = file["parameters"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["name"] == own)
        .unwrap();
    parameter["expression"] = json!(format!("{reference} * 5"));
    let mut opened = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    opened.recompute();
    let feature = status(&opened);
    let error = feature["error"]
        .as_str()
        .unwrap_or_else(|| panic!("Sketch1 does not fail: {feature}"));
    assert!(
        error.contains(&format!("the pattern {pattern}"))
            && error.contains(&format!("driven dimension {k} ({reference})"))
            && error.contains("would depend on itself"),
        "{error}"
    );
}

/// An offset's distance still may not use its own sketch's driven
/// dimension: the refusal names the offset distance in words.
#[test]
fn an_offset_distance_is_named_in_the_refusal() {
    let mut d = measured_rectangle(true);
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": ["c1", "c2", "c3", "c4"],
               "distance": "d3 / 20"}),
    );
    assert!(
        error.contains("d3 is what the driven dimension k7 of Sketch1 measures")
            && error.contains("Sketch1's offset distance cannot use it")
            && error.contains(
                "only its dimensions, pattern spacings and angles, and the features after \
                 Sketch1 can"
            ),
        "{error}"
    );
    assert!(!error.contains("offsets["), "{error}");
}
