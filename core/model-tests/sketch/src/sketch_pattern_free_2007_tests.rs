// SPDX-License-Identifier: MIT
//! Deleting a sketch pattern's copy, trimming or rounding one, and freeing
//! the pattern (mitcad#2007, GitHub mitcad/Mitcad#432).
//!
//! A pattern made its copies again after every edit (`sync_patterns`): a
//! deleted copy came back, and a trimmed one was made whole again with a
//! stray piece left. Now a deleted copy leaves its instance out of the
//! pattern (the record's `skipped`), which stays editable; Trim, Extend,
//! Sketch Fillet and Chamfer on a pattern's curve first free that pattern
//! into plain sketch geometry, with a note; and `sketch.remove` of the
//! pattern (the context menu's Free Pattern) leaves every curve where it
//! was.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const FREED: &str = "the pattern's copies are now plain sketch geometry";

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

fn id(v: &Value) -> String {
    v.as_str()
        .unwrap_or_else(|| panic!("no id: {v}"))
        .to_owned()
}

fn find<'a>(s: &'a Value, id: &str) -> Option<&'a Value> {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
}

fn entity<'a>(s: &'a Value, id: &str) -> &'a Value {
    find(s, id).unwrap_or_else(|| panic!("no {id} in {s}"))
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

/// A line's solved ends.
fn ends(s: &Value, id: &str) -> ([f64; 2], [f64; 2]) {
    let g = &entity(s, id)["geometry"];
    (xy(&g["start"]), xy(&g["end"]))
}

/// Whether a line runs between `a` and `b`, either way round.
fn runs(s: &Value, id: &str, a: [f64; 2], b: [f64; 2]) -> bool {
    let (p, q) = ends(s, id);
    (near(p, a) && near(q, b)) || (near(p, b) && near(q, a))
}

fn shown_notes(result: &Value) -> Vec<String> {
    result["note_messages"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn patterns(s: &Value) -> Vec<Value> {
    s["patterns"].as_array().cloned().unwrap_or_default()
}

/// The pattern's instances made, by index.
fn instances(s: &Value) -> Vec<[u64; 2]> {
    patterns(s)[0]["copies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            [
                c["index"][0].as_u64().unwrap(),
                c["index"][1].as_u64().unwrap(),
            ]
        })
        .collect()
}

/// The copy of `original` in the instance at `index`.
fn copy_of(s: &Value, index: [u32; 2], original: &str) -> String {
    let copy = patterns(s)[0]["copies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["index"] == json!(index))
        .unwrap_or_else(|| panic!("no instance {index:?} in {s}"))
        .clone();
    id(&copy["entities"][original])
}

/// Every point's solved place, by id.
fn points(s: &Value) -> BTreeMap<String, [f64; 2]> {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "point")
        .map(|e| (id(&e["id"]), xy(&e["at"])))
        .collect()
}

/// The curves' ids.
fn curves(s: &Value) -> Vec<String> {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] != "point")
        .map(|e| id(&e["id"]))
        .collect()
}

fn line(d: &mut Document<MockKernel>, start: Value, end: Value) -> String {
    id(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
    )["made"][0])
}

/// A sketch with a line from (0, 0) to (10, 0) in a rectangular pattern
/// of `count` along x, 20 mm apart: copies from (20, 0), (40, 0), …. The
/// line and the pattern's id.
fn rectangular(count: u32) -> (Document<MockKernel>, String, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let original = line(&mut d, json!([0, 0]), json!([10, 0]));
    let made = command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": [original],
               "direction": [1, 0], "count": [count, 1], "spacing": [20, 10]}),
    );
    let pattern = id(&made["constraints"][0]);
    (d, original, pattern)
}

/// A sketch with a line from (10, 0) to (20, 0) in a circular pattern of
/// six about the origin. The line and the pattern's id.
fn circular() -> (Document<MockKernel>, String, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let original = line(&mut d, json!([10, 0]), json!([20, 0]));
    let made = command(
        &mut d,
        json!({"cmd": "sketch.circular_pattern", "sketch": "F1", "entities": [original],
               "center": [0, 0], "count": 6}),
    );
    let pattern = id(&made["constraints"][0]);
    (d, original, pattern)
}

/// The copy at the instance `k` of [`circular`] runs at k × 60°.
fn turned(k: u32) -> ([f64; 2], [f64; 2]) {
    let a = (60.0 * f64::from(k)).to_radians();
    (
        [10.0 * a.cos(), 10.0 * a.sin()],
        [20.0 * a.cos(), 20.0 * a.sin()],
    )
}

#[test]
fn deleting_a_copy_of_a_rectangular_pattern_leaves_its_instance_out() {
    let (mut d, original, pattern) = rectangular(4);
    let s = sketch(&d);
    assert_eq!(instances(&s).len(), 3, "{s}");
    let gone = copy_of(&s, [2, 0], &original);
    let kept = copy_of(&s, [3, 0], &original);
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [gone]}),
    );
    command(&mut d, json!({"cmd": "recompute"}));
    let s = sketch(&d);
    // The copy stays deleted; the pattern lists its instance as skipped.
    assert!(find(&s, &gone).is_none(), "{s}");
    assert_eq!(patterns(&s)[0]["skipped"], json!([[2, 0]]), "{s}");
    assert_eq!(instances(&s), [[1, 0], [3, 0]], "{s}");
    assert!(runs(&s, &kept, [60.0, 0.0], [70.0, 0.0]), "{s}");
    // Saved and opened again, it is still left out.
    let mut reopened = Document::from_json(&d.to_json(), MockKernel::default()).unwrap();
    reopened.recompute();
    let r = sketch(&reopened);
    assert_eq!(patterns(&r)[0]["skipped"], json!([[2, 0]]), "{r}");
    assert_eq!(instances(&r), [[1, 0], [3, 0]], "{r}");

    // Edit Pattern still works and keeps the skip: a fifth instance comes,
    // the third does not.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "count": [5, 1], "spacing": [25, 10]}),
    );
    let s = sketch(&d);
    assert_eq!(instances(&s), [[1, 0], [3, 0], [4, 0]], "{s}");
    assert_eq!(patterns(&s)[0]["skipped"], json!([[2, 0]]), "{s}");
    let fifth = copy_of(&s, [4, 0], &original);
    assert!(runs(&s, &fifth, [100.0, 0.0], [110.0, 0.0]), "{s}");
    assert!(runs(&s, &kept, [75.0, 0.0], [85.0, 0.0]), "{s}");

    // Fewer instances than the skipped one's place drop the skip; more
    // again make that instance anew.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "count": [2, 1]}),
    );
    let s = sketch(&d);
    assert_eq!(instances(&s), [[1, 0]], "{s}");
    assert!(patterns(&s)[0].get("skipped").is_none(), "{s}");
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "count": [4, 1]}),
    );
    let s = sketch(&d);
    assert_eq!(instances(&s), [[1, 0], [2, 0], [3, 0]], "{s}");
}

#[test]
fn deleting_a_copy_of_a_circular_pattern_by_a_point_leaves_its_instance_out() {
    let (mut d, original, pattern) = circular();
    let s = sketch(&d);
    let start = id(&entity(&s, &original)["start"]);
    // The copy of the original's start in the fourth instance: deleting it
    // takes its line (and the line's other end) along.
    let point = copy_of(&s, [3, 0], &start);
    let curve = copy_of(&s, [3, 0], &original);
    let other = copy_of(&s, [1, 0], &original);
    let removed = command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [point]}),
    );
    let s = sketch(&d);
    assert!(find(&s, &point).is_none(), "{removed}");
    assert!(find(&s, &curve).is_none(), "{s}");
    assert_eq!(patterns(&s)[0]["skipped"], json!([[3, 0]]), "{s}");
    assert_eq!(instances(&s), [[1, 0], [2, 0], [4, 0], [5, 0]], "{s}");
    let (a, b) = turned(1);
    assert!(runs(&s, &other, a, b), "{s}");

    // A curve of another copy deleted: that instance goes too.
    let fifth = copy_of(&s, [5, 0], &original);
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [fifth]}),
    );
    let s = sketch(&d);
    assert_eq!(patterns(&s)[0]["skipped"], json!([[3, 0], [5, 0]]), "{s}");
    assert_eq!(instances(&s), [[1, 0], [2, 0], [4, 0]], "{s}");

    // Edit Pattern: a smaller angle moves the copies that are made and
    // keeps the skips.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "angle": "300 deg"}),
    );
    let s = sketch(&d);
    assert_eq!(instances(&s), [[1, 0], [2, 0], [4, 0]], "{s}");
    let (a, b) = turned(1);
    assert!(runs(&s, &other, a, b), "{s}");

    // Undo brings the deleted copy back.
    command(&mut d, json!({"cmd": "undo"}));
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert_eq!(patterns(&s)[0]["skipped"], json!([[3, 0]]), "{s}");
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert!(patterns(&s)[0].get("skipped").is_none(), "{s}");
    assert_eq!(instances(&s).len(), 5, "{s}");
    let (a, b) = turned(3);
    assert!(runs(&s, &curve, a, b), "{s}");
}

#[test]
fn trimming_a_copy_frees_the_pattern_and_trims_it() {
    let (mut d, original, _) = rectangular(3);
    let s = sketch(&d);
    let cut = copy_of(&s, [1, 0], &original);
    let other = copy_of(&s, [2, 0], &original);
    // A line across the first copy at x = 25.
    line(&mut d, json!([25, -5]), json!([25, 5]));
    let trimmed = command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": cut, "at": [28, 0]}),
    );
    assert!(
        shown_notes(&trimmed).iter().any(|n| n == FREED),
        "{trimmed}"
    );
    let s = sketch(&d);
    assert!(patterns(&s).is_empty(), "{s}");
    // The copy is cut at the crossing and stays cut; the others keep their
    // ids and places.
    assert!(runs(&s, &cut, [20.0, 0.0], [25.0, 0.0]), "{s}");
    assert!(runs(&s, &other, [40.0, 0.0], [50.0, 0.0]), "{s}");
    assert!(runs(&s, &original, [0.0, 0.0], [10.0, 0.0]), "{s}");
    // The original no longer moves the copies.
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": original}, "value": 5}),
    );
    let s = sketch(&d);
    assert!(runs(&s, &cut, [20.0, 0.0], [25.0, 0.0]), "{s}");
    assert!(runs(&s, &other, [40.0, 0.0], [50.0, 0.0]), "{s}");

    // Undo brings the pattern back, with the copy whole.
    command(&mut d, json!({"cmd": "undo"}));
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert_eq!(patterns(&s).len(), 1, "{s}");
    assert_eq!(copy_of(&s, [1, 0], &original), cut);
    assert!(runs(&s, &cut, [20.0, 0.0], [30.0, 0.0]), "{s}");
}

#[test]
fn rounding_a_copys_corner_frees_the_pattern() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    // Two lines from a shared corner at the origin, patterned 30 mm apart.
    let a = line(&mut d, json!([0, 0]), json!([10, 0]));
    let corner = id(&entity(&sketch(&d), &a)["start"]);
    let b = line(&mut d, json!(corner), json!([0, 10]));
    command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": [a, b],
               "direction": [1, 0], "count": [2, 1], "spacing": [30, 10]}),
    );
    let s = sketch(&d);
    let (ca, cb) = (copy_of(&s, [1, 0], &a), copy_of(&s, [1, 0], &b));
    let before = curves(&s).len();
    let rounded = command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": ca, "b": cb, "radius": 2}),
    );
    assert!(
        shown_notes(&rounded).iter().any(|n| n == FREED),
        "{rounded}"
    );
    let s = sketch(&d);
    assert!(patterns(&s).is_empty(), "{s}");
    // One arc more; the original corner is not rounded.
    assert_eq!(curves(&s).len(), before + 1, "{s}");
    assert!(runs(&s, &a, [0.0, 0.0], [10.0, 0.0]), "{s}");
}

#[test]
fn freeing_a_pattern_keeps_every_curve_where_it_was() {
    let (mut d, original, pattern) = circular();
    // A constraint of the user's on the original stays.
    command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": original}}),
    );
    let s = sketch(&d);
    let constraints = s["constraints"].as_array().unwrap().len();
    let before_points = points(&s);
    let before_curves = curves(&s);
    let copies: Vec<String> = (1..6).map(|k| copy_of(&s, [k, 0], &original)).collect();
    // Free Pattern sends `sketch.remove` of the pattern.
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [pattern]}),
    );
    let s = sketch(&d);
    assert!(patterns(&s).is_empty(), "{s}");
    assert_eq!(curves(&s), before_curves, "{s}");
    let after = points(&s);
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        before_points.keys().collect::<Vec<_>>()
    );
    for (p, at) in &before_points {
        assert!(near(after[p], *at), "{p}: {:?} vs {at:?}", after[p]);
    }
    assert_eq!(
        s["constraints"].as_array().unwrap().len(),
        constraints,
        "{s}"
    );
    for (k, c) in copies.iter().enumerate() {
        let (a, b) = turned(k as u32 + 1);
        assert!(runs(&s, c, a, b), "{s}");
    }
    // A copy is plain geometry now: deleting it takes nothing else.
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [copies[0]]}),
    );
    let s = sketch(&d);
    assert_eq!(curves(&s).len(), before_curves.len() - 1, "{s}");

    // Undo brings the pattern back with every copy.
    command(&mut d, json!({"cmd": "undo"}));
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert_eq!(patterns(&s).len(), 1, "{s}");
    assert_eq!(instances(&s).len(), 5, "{s}");
}
