// SPDX-License-Identifier: MIT
//! What the Sketch Palette's Show Unconstrained selects (mitcad#2026,
//! GitHub mitcad/Mitcad#413): the sketch query flags every curve and point
//! that is not fully constrained, and lists only the others in
//! `fully_constrained`, so that the app selects exactly the geometry that
//! can still move.

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

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The ids of the entities whose `fully_constrained` flag is false, as the
/// app reads them (SketchModel's `constrained`), sorted.
fn unconstrained(s: &Value) -> Vec<String> {
    let mut ids: Vec<String> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["fully_constrained"] != json!(true))
        .map(|e| e["id"].as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    ids
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// A new document with Sketch1 (F1): a line whose ends are fixed, and a
/// free line beside it. Returns the ids each line's command made.
fn fixed_and_free() -> (Document<MockKernel>, Vec<String>, Vec<String>) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let fixed = strings(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [20, 0]}),
        )["entities"],
    );
    let free = strings(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 10], "end": [20, 10]}),
        )["entities"],
    );
    assert_eq!(fixed.len(), 3, "{fixed:?}");
    assert_eq!(free.len(), 3, "{free:?}");
    // Its two points fixed: the first line cannot move.
    let points: Vec<&String> = fixed.iter().filter(|id| id.starts_with('p')).collect();
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": points}),
    );
    (d, fixed, free)
}

#[test]
fn a_free_line_and_its_points_are_all_that_is_not_fully_constrained() {
    let (d, fixed, free) = fixed_and_free();
    let s = sketch(&d, "F1");
    // The free line's two points, two degrees of freedom each.
    assert_eq!(s["dof"], 4, "{s}");
    assert_eq!(unconstrained(&s), sorted(free.clone()), "{s}");
    let listed = sorted(strings(&s["fully_constrained"]));
    for id in &fixed {
        assert!(listed.contains(id), "{id} not in {listed:?}");
    }
    for id in &free {
        assert!(!listed.contains(id), "{id} in {listed:?}");
    }
}

#[test]
fn a_line_held_in_part_still_moves_and_its_fixed_end_does_not() {
    let (mut d, _, free) = fixed_and_free();
    let line = free.iter().find(|id| id.starts_with('c')).unwrap().clone();
    let points: Vec<String> = free
        .iter()
        .filter(|id| id.starts_with('p'))
        .cloned()
        .collect();
    // Horizontal, with its start fixed: the end still slides along it.
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": line}}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [points[0]]}),
    );
    let s = sketch(&d, "F1");
    assert_eq!(s["dof"], 1, "{s}");
    assert_eq!(
        unconstrained(&s),
        sorted(vec![line.clone(), points[1].clone()]),
        "{s}"
    );
    // Fully constrained once the length is given: nothing left to select.
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": line}, "value": 20}),
    );
    let s = sketch(&d, "F1");
    assert_eq!(s["dof"], 0, "{s}");
    assert!(unconstrained(&s).is_empty(), "{s}");
}
