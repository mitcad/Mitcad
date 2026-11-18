// SPDX-License-Identifier: MIT
//! A constraint the others imply only in part (mitcad#1992, GitHub
//! mitcad/Mitcad#420): a Symmetry of an arc's ends about an axis through
//! the arc's centre. The ends are equally far from the centre, so ends
//! mirrored across the axis are also level and the reverse: one of the
//! Symmetry's two equations follows from the arc's own, while the other
//! still takes a degree of freedom away. The sketch lists it in
//! `redundant` (the solver's view) and in `partly_redundant` with the
//! degrees of freedom it takes away, and tells it as information, not as a
//! warning that it follows from the others. A constraint the others imply
//! in full still warns.

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

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
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

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The notices of a level: their messages.
fn notices(s: &Value, level: &str) -> Vec<String> {
    s["notices"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|n| n["level"] == level)
                .map(|n| n["message"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// A sketch with a construction line along the y axis from a fixed point
/// at the origin (as the sketch's axis), and a construction arc about that
/// point of radius 50 whose ends are not mirrored across it: (30, 40) and
/// (-40, 30). Three degrees of freedom: the arc's ends on its circle.
fn arc_about_the_axis() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
               "entities": [
                   {"id": "p1", "type": "point", "at": [0.0, 0.0], "fixed": true},
                   {"id": "p2", "type": "point", "at": [0.0, 100.0], "fixed": true},
                   {"id": "c3", "type": "line", "start": "p1", "end": "p2",
                    "construction": true, "fixed": true},
                   {"id": "p4", "type": "point", "at": [30.0, 40.0]},
                   {"id": "p5", "type": "point", "at": [-40.0, 30.0]},
                   {"id": "c6", "type": "arc", "center": "p1", "start": "p4", "end": "p5",
                    "construction": true}
               ],
               "constraints": [], "dimensions": []}}),
    );
    d
}

#[test]
fn a_symmetry_of_an_arc_s_ends_about_its_centre_s_axis_is_information() {
    let mut d = arc_about_the_axis();
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["dof"], 3, "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    // The reporter's step: Symmetry of the arc's ends about the axis.
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "symmetric", "a": "p5", "b": "p4", "axis": "c3"}}),
    );
    let k = added["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    // It took a degree of freedom away: the ends level and mirrored.
    assert_eq!(s["dof"], 2, "{s}");
    let (a, b) = (at(&s, "p4"), at(&s, "p5"));
    assert!(close(a[0], -b[0]) && close(a[1], b[1]), "{a:?} {b:?}: {s}");
    assert!(close(a[0].hypot(a[1]), b[0].hypot(b[1])), "{s}");
    // The solver finds one of its equations implied: in `redundant` for
    // agents, with what it still takes away in `partly_redundant`.
    assert_eq!(s["redundant"], json!([k]), "{s}");
    assert_eq!(s["partly_redundant"], json!({k.as_str(): 1}), "{s}");
    // No warning that it follows from the others; information that it
    // partly repeats them, about the constraint, without ids.
    for w in strings(&s["warning_messages"])
        .iter()
        .chain(&notices(&s, "warning"))
    {
        assert!(!w.contains("follow"), "{w}: {s}");
    }
    let info = notices(&s, "info");
    assert_eq!(info.len(), 1, "{s}");
    assert!(
        info[0].ends_with("partly repeats the others; it still removes one degree of freedom"),
        "{info:?}"
    );
    assert!(info[0].contains("Symmetry"), "{info:?}");
    assert!(!info[0].contains(k.as_str()), "{info:?}");
    let about: Vec<&Value> = s["notices"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["level"] == "info")
        .map(|n| &n["target"])
        .collect();
    assert_eq!(about, [&json!(k)], "{s}");
}

#[test]
fn a_constraint_the_others_imply_in_full_still_warns() {
    // A line made horizontal twice: the second follows from the first and
    // takes nothing away.
    let mut d = Document::new(MockKernel::default());
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
               "entities": [
                   {"id": "p1", "type": "point", "at": [0.0, 0.0]},
                   {"id": "p2", "type": "point", "at": [40.0, 1.0]},
                   {"id": "c3", "type": "line", "start": "p1", "end": "p2"}
               ],
               "constraints": [
                   {"id": "k4", "type": "horizontal", "line": "c3"},
                   {"id": "k5", "type": "horizontal", "line": "c3"}
               ],
               "dimensions": []}}),
    );
    let s = sketch(&d, "F1");
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["dof"], 3, "{s}");
    assert_eq!(s["redundant"], json!(["k5"]), "{s}");
    assert!(s["partly_redundant"].is_null(), "{s}");
    let warnings = strings(&s["warning_messages"]);
    assert!(
        warnings
            .iter()
            .any(|w| w.starts_with("Redundant:") && w.contains("follows from")),
        "{s}"
    );
    assert!(
        notices(&s, "warning")
            .iter()
            .any(|w| w.contains("follows from")),
        "{s}"
    );
    assert!(notices(&s, "info").is_empty(), "{s}");
    // And the command refuses one more.
    let error = d
        .command(
            &json!({"cmd": "sketch.add_constraint", "sketch": "F1",
                    "constraint": {"type": "horizontal", "line": "c3"}})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("would over-constrain"), "{error}");
}
