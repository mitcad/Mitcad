// SPDX-License-Identifier: MIT
//! Auto Constrain (mitcad#2019): `sketch.auto_constrain` adds the relations
//! a sketch's geometry has within its tolerances, then dimensions with the
//! current values for what is left free, in one undo step, moving nothing
//! beyond the length tolerance and adding nothing redundant or conflicting.

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

fn new_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
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

/// A line with points of its own at both ends: no constraint.
fn line(d: &mut Document<MockKernel>, start: [f64; 2], end: [f64; 2]) -> String {
    curve_made(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
    ))
}

/// A counter-clockwise arc about `center` from `start` toward `end`.
fn arc(d: &mut Document<MockKernel>, center: [f64; 2], start: [f64; 2], end: [f64; 2]) -> String {
    curve_made(&command(
        d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": center,
               "start": start, "end": end}),
    ))
}

/// A closed outline of lines through `corners`, each with points of its
/// own.
fn outline(d: &mut Document<MockKernel>, corners: &[[f64; 2]]) -> Vec<String> {
    (0..corners.len())
        .map(|i| line(d, corners[i], corners[(i + 1) % corners.len()]))
        .collect()
}

fn auto(d: &mut Document<MockKernel>, extra: Value) -> Value {
    let mut cmd = json!({"cmd": "sketch.auto_constrain", "sketch": "F1"});
    for (k, v) in extra.as_object().unwrap() {
        cmd[k] = v.clone();
    }
    command(d, cmd)
}

/// Every point's position, by id.
fn positions(s: &Value) -> Vec<(String, [f64; 2])> {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "point")
        .map(|e| {
            (
                e["id"].as_str().unwrap().to_owned(),
                [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()],
            )
        })
        .collect()
}

/// The points still there moved no more than `tol`.
fn assert_within(after: &[(String, [f64; 2])], before: &[(String, [f64; 2])], tol: f64) {
    for (id, a) in after {
        if let Some((_, b)) = before.iter().find(|(i, _)| i == id) {
            assert!(
                (a[0] - b[0]).hypot(a[1] - b[1]) <= tol,
                "{id} moved from {b:?} to {a:?}"
            );
        }
    }
}

/// Nothing redundant or in conflict.
fn assert_clean(s: &Value) {
    // The sketch query has the status at its top level.
    assert_eq!(s["redundant"], json!([]), "{}", s["redundant"]);
    assert_eq!(s["conflicts"], json!([]), "{}", s["conflicts"]);
}

fn added(result: &Value, kind: &str) -> u64 {
    result["added"][kind].as_u64().unwrap_or(0)
}

fn count(s: &Value, list: &str, kind: &str) -> usize {
    s[list].as_array().map_or(0, |items| {
        items.iter().filter(|i| i["type"] == kind).count()
    })
}

#[test]
fn a_rectangle_drawn_without_constraints_ends_fully_constrained() {
    let mut d = new_sketch();
    outline(
        &mut d,
        &[[10.0, 5.0], [40.0, 5.0], [40.0, 25.0], [10.0, 25.0]],
    );
    let before = positions(&sketch(&d));
    assert_eq!(before.len(), 8);
    let result = auto(&mut d, json!({}));
    assert_eq!(result["dof"], 0, "{result}");
    assert_eq!(result["status"]["dof"], 0, "{result}");
    // The corners joined, two lines level and two upright.
    assert_eq!(result["joined"], 4, "{result}");
    assert_eq!(added(&result, "coincident"), 4, "{result}");
    assert_eq!(added(&result, "horizontal"), 2, "{result}");
    assert_eq!(added(&result, "vertical"), 2, "{result}");
    // Two sizes and two positions.
    assert_eq!(added(&result, "length"), 2, "{result}");
    assert_eq!(added(&result, "horizontal_distance"), 1, "{result}");
    assert_eq!(added(&result, "vertical_distance"), 1, "{result}");
    assert_eq!(result["dimensions"].as_array().unwrap().len(), 4);
    assert_eq!(result["relations"], 8);
    assert_eq!(
        result["summary"],
        "8 constraints, 4 dimensions; the sketch is fully constrained"
    );
    let s = sketch(&d);
    assert_clean(&s);
    assert_eq!(positions(&s).len(), 5, "four corners and the origin");
    assert_within(&positions(&s), &before, 1e-9);
    // The values are the geometry's, shown apart from each other.
    let texts: Vec<[f64; 2]> = s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            [
                d["text"][0].as_f64().unwrap(),
                d["text"][1].as_f64().unwrap(),
            ]
        })
        .collect();
    for (i, a) in texts.iter().enumerate() {
        for b in &texts[i + 1..] {
            assert!((a[0] - b[0]).hypot(a[1] - b[1]) > 1.0, "{texts:?}");
        }
    }
    // One undo step takes it all back.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert_eq!(positions(&s), before);
    assert_eq!(s["constraints"].as_array().map_or(0, Vec::len), 0);
    assert_eq!(s["dimensions"].as_array().map_or(0, Vec::len), 0);
}

#[test]
fn a_slot_gets_its_tangents_and_equal_radii() {
    let mut d = new_sketch();
    // Two half circles of radius 5 joined by two level lines.
    line(&mut d, [20.0, 15.0], [50.0, 15.0]);
    line(&mut d, [50.0, 5.0], [20.0, 5.0]);
    arc(&mut d, [50.0, 10.0], [50.0, 5.0], [50.0, 15.0]);
    arc(&mut d, [20.0, 10.0], [20.0, 15.0], [20.0, 5.0]);
    let before = positions(&sketch(&d));
    let result = auto(&mut d, json!({}));
    assert_eq!(result["joined"], 4, "{result}");
    assert_eq!(added(&result, "tangent"), 4, "{result}");
    assert_eq!(added(&result, "equal"), 1, "{result}");
    assert_eq!(result["dof"], 0, "{result}");
    let s = sketch(&d);
    assert_clean(&s);
    assert_eq!(count(&s, "constraints", "tangent"), 4);
    assert_eq!(count(&s, "constraints", "equal"), 1);
    // One radius, one length, two positions.
    assert_eq!(added(&result, "radius"), 1, "{result}");
    assert_eq!(added(&result, "length"), 1, "{result}");
    assert_within(&positions(&s), &before, 1e-9);
}

#[test]
fn near_relations_within_the_tolerance_hold_and_those_outside_do_not() {
    let mut d = new_sketch();
    // An outline as an import leaves it: a nearly level line, ends 0.005 mm
    // apart, an upright line 0.005 mm off, a line 1.9 degrees off level and
    // a gap of 0.05 mm, beyond the tolerance of 0.01 mm.
    line(&mut d, [10.0, 10.0], [40.0, 10.004]);
    line(&mut d, [40.005, 10.002], [39.995, 30.0]);
    line(&mut d, [39.995, 30.0], [10.0, 31.0]);
    line(&mut d, [10.0, 31.05], [10.0, 10.0]);
    let before = positions(&sketch(&d));
    let result = auto(
        &mut d,
        json!({"length_tolerance": 0.01, "angle_tolerance": 0.5}),
    );
    assert_eq!(result["joined"], 3, "{result}");
    assert_eq!(added(&result, "horizontal"), 1, "{result}");
    assert_eq!(added(&result, "vertical"), 2, "{result}");
    let s = sketch(&d);
    assert_clean(&s);
    // The open corner stays open (its end held on the upright line): five
    // points of the outline and the origin.
    assert_eq!(positions(&s).len(), 6, "{}", s["entities"]);
    // The slanted line got no direction.
    assert_eq!(count(&s, "constraints", "horizontal"), 1);
    assert_within(&positions(&s), &before, 0.01);
    assert_eq!(result["dof"], 0, "{result}");
}

#[test]
fn relations_only_leave_the_free_degrees_and_say_so() {
    let mut d = new_sketch();
    outline(
        &mut d,
        &[[10.0, 5.0], [40.0, 5.0], [40.0, 25.0], [10.0, 25.0]],
    );
    let result = auto(&mut d, json!({"dimensions": false}));
    assert_eq!(result["dimensions"], json!([]));
    assert_eq!(result["dof"], 4, "{result}");
    assert_eq!(
        result["summary"],
        "8 constraints, 0 dimensions; 4 degrees of freedom left"
    );
    // No origin made for nothing.
    assert_eq!(positions(&sketch(&d)).len(), 4);
    // Asked again with dimensions, it finishes the job.
    let result = auto(&mut d, json!({"relations": false}));
    assert_eq!(result["relations"], 0, "{result}");
    assert_eq!(result["dof"], 0, "{result}");
    assert_clean(&sketch(&d));
}

#[test]
fn the_selection_limits_what_is_constrained() {
    let mut d = new_sketch();
    let first = outline(
        &mut d,
        &[[10.0, 5.0], [40.0, 5.0], [40.0, 25.0], [10.0, 25.0]],
    );
    let second = outline(
        &mut d,
        &[[60.0, 5.0], [80.0, 5.0], [80.0, 15.0], [60.0, 15.0]],
    );
    let result = auto(&mut d, json!({"entities": first}));
    assert_eq!(result["joined"], 4, "{result}");
    let s = sketch(&d);
    assert_clean(&s);
    // The second rectangle has none.
    for c in s["constraints"].as_array().unwrap() {
        let text = c.to_string();
        for line in &second {
            assert!(!text.contains(&format!("\"{line}\"")), "{text}");
        }
    }
    assert_eq!(result["dof"], 16, "the second rectangle's 16: {result}");
}

#[test]
fn symmetric_circles_about_a_centre_line_and_a_point_at_the_origin() {
    let mut d = new_sketch();
    // A construction line upright through x = 30, two circles mirrored
    // about it, and a line from the origin.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [30.0, 0.0],
               "end": [30.0, 40.0], "construction": true}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [20.0, 20.0], "radius": 4.0}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [40.0, 20.0], "radius": 4.0}),
    );
    line(&mut d, [0.0, 0.0], [10.0, 7.0]);
    let result = auto(&mut d, json!({}));
    assert_eq!(added(&result, "symmetric"), 1, "{result}");
    assert_eq!(added(&result, "equal"), 1, "{result}");
    assert!(added(&result, "coincident") >= 1, "{result}");
    assert_eq!(result["dof"], 0, "{result}");
    assert_clean(&sketch(&d));
}

#[test]
fn a_fully_constrained_sketch_gets_nothing_more() {
    let mut d = new_sketch();
    outline(
        &mut d,
        &[[10.0, 5.0], [40.0, 5.0], [40.0, 25.0], [10.0, 25.0]],
    );
    auto(&mut d, json!({}));
    let s = sketch(&d);
    let result = auto(&mut d, json!({}));
    assert_eq!(result["relations"], 0, "{result}");
    assert_eq!(result["dimensions"], json!([]));
    assert_eq!(sketch(&d)["constraints"], s["constraints"]);
}

#[test]
fn bad_options_are_refused_in_words() {
    let mut d = new_sketch();
    line(&mut d, [0.0, 0.0], [10.0, 0.0]);
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.auto_constrain", "sketch": "F1", "relations": false,
               "dimensions": false}),
    );
    assert!(error.contains("relations, dimensions or both"), "{error}");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.auto_constrain", "sketch": "F1", "length_tolerance": 0}),
    );
    assert!(error.contains("length tolerance"), "{error}");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.auto_constrain", "sketch": "F1", "angle_tolerance": 60}),
    );
    assert!(error.contains("45 degrees"), "{error}");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.auto_constrain", "sketch": "F1", "entities": ["c99"]}),
    );
    assert!(error.contains("no longer in the sketch"), "{error}");
}
