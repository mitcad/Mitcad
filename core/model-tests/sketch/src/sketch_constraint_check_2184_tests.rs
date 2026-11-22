// SPDX-License-Identifier: MIT
//! `sketch.add_constraint` with `"check": true` (mitcad#2184), with the
//! mock kernel: nothing changes, not the sketch, its degrees of freedom
//! or the undo history; the answer is `{"check": true}` when the
//! constraint would be added, else the command is refused with the words
//! the plain command would give. Horizontal/Vertical asks with it which
//! direction it can add: two points whose heights are held already are
//! refused level and taken plumb, two points level on one line are refused
//! level (implied) and taken plumb, and two fixed points are refused both
//! ways.

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

fn sketch(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "sketch", "uid": "F1"}))
}

fn undo_depth(doc: &Document<MockKernel>) -> i64 {
    query(doc, json!({"query": "document"}))["undo_depth"]
        .as_i64()
        .unwrap()
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    let e = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"));
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

fn pair(kind: &str, a: &str, b: &str, check: bool) -> Value {
    json!({"cmd": "sketch.add_constraint", "sketch": "F1", "check": check,
           "constraint": {"type": kind, "a": a, "b": b}})
}

/// A sketch on XY: the fixed construction line c5 along y = 0 from p3
/// (-30, 0) to p4 (30, 0), the point p2 at (10, 0) held on it (fixed
/// instead with `p2_fixed`), and the point p1 at `p1`, fixed with
/// `p1_fixed` and held on the line with `p1_on_line`.
fn design(p1: [f64; 2], p1_fixed: bool, p1_on_line: bool, p2_fixed: bool) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let mut constraints = Vec::new();
    if !p2_fixed {
        constraints.push(json!({"id": "k6", "type": "coincident", "point": "p2", "entity": "c5"}));
    }
    if p1_on_line {
        constraints.push(json!({"id": "k7", "type": "coincident", "point": "p1", "entity": "c5"}));
    }
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
            "entities": [
                {"id": "p1", "type": "point", "at": p1, "fixed": p1_fixed},
                {"id": "p2", "type": "point", "at": [10, 0], "fixed": p2_fixed},
                {"id": "p3", "type": "point", "at": [-30, 0], "fixed": true},
                {"id": "p4", "type": "point", "at": [30, 0], "fixed": true},
                {"id": "c5", "type": "line", "start": "p3", "end": "p4", "construction": true}],
            "constraints": constraints}}),
    );
    doc
}

/// The sketch's answer without what a solve may vary in the last digits:
/// its constraints, its degrees of freedom and every point's place.
fn state(doc: &Document<MockKernel>) -> (Value, Value, Vec<[f64; 2]>) {
    let s = sketch(doc);
    let points = ["p1", "p2", "p3", "p4"].iter().map(|p| at(&s, p)).collect();
    (s["constraints"].clone(), s["dof"].clone(), points)
}

#[test]
fn a_check_changes_nothing_and_says_what_the_command_would_do() {
    // p1 fixed above the line, p2 on it: their heights are held, so level
    // conflicts and plumb is what can be added.
    let mut doc = design([-10.0, 4.0], true, false, false);
    assert_eq!(sketch(&doc)["dof"], 1);
    let depth = undo_depth(&doc);
    let before = state(&doc);

    let checked = rejected(&mut doc, pair("horizontal_points", "p1", "p2", true));
    assert_eq!(state(&doc), before, "a refused check changes nothing");
    assert_eq!(undo_depth(&doc), depth);
    let refused = rejected(&mut doc, pair("horizontal_points", "p1", "p2", false));
    assert_eq!(checked, refused, "the check is refused as the command is");

    let answer = command(&mut doc, pair("vertical_points", "p1", "p2", true));
    assert_eq!(answer["check"], true, "{answer}");
    assert!(answer.get("constraints").is_none(), "{answer}");
    assert_eq!(
        state(&doc),
        before,
        "a check that would add changes nothing"
    );
    assert_eq!(undo_depth(&doc), depth, "no undo step");

    // Added for real: p2 slides under p1, the sketch fully constrained.
    command(&mut doc, pair("vertical_points", "p1", "p2", false));
    let s = sketch(&doc);
    assert_eq!(s["dof"], 0);
    let p2 = at(&s, "p2");
    assert!((p2[0] + 10.0).abs() < 1e-9 && p2[1].abs() < 1e-9, "{p2:?}");
    assert_eq!(undo_depth(&doc), depth + 1);
}

#[test]
fn an_implied_alignment_is_refused_by_the_check_and_the_other_taken() {
    // Both points held on the line: level is implied.
    let mut doc = design([-10.0, 0.0], false, true, false);
    assert_eq!(sketch(&doc)["dof"], 2);
    let before = state(&doc);
    rejected(&mut doc, pair("horizontal_points", "p1", "p2", true));
    let answer = command(&mut doc, pair("vertical_points", "p1", "p2", true));
    assert_eq!(answer["check"], true, "{answer}");
    assert_eq!(state(&doc), before);
}

#[test]
fn two_fixed_points_are_refused_both_ways() {
    let mut doc = design([-10.0, 4.0], true, false, true);
    let depth = undo_depth(&doc);
    let before = state(&doc);
    rejected(&mut doc, pair("horizontal_points", "p1", "p2", true));
    rejected(&mut doc, pair("vertical_points", "p1", "p2", true));
    assert_eq!(state(&doc), before);
    assert_eq!(undo_depth(&doc), depth);
}
