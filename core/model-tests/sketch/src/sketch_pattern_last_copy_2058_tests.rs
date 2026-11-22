// SPDX-License-Identifier: MIT
//! The last copy of a sketch pattern deleted takes its pattern
//! (mitcad#2058, GitHub mitcad/Mitcad#471).
//!
//! Deleting a copy leaves its instance out of the pattern (mitcad#2007).
//! With every copy deleted while the originals stayed, the record was kept
//! with every instance skipped: it made nothing, yet it still used its
//! spacing (or angle) parameters, which then could not be deleted. Now
//! such a pattern goes, as Free Pattern removes it, in the same command
//! and undo step, the edit releases its parameters, and the result's notes
//! say so. A design saved with such a record loses it on the next edit of
//! its sketch, not when it is opened.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const REMOVED: &str = "the pattern was removed with its last copy";

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

/// The names in the `parameters` query.
fn parameters(doc: &Document<MockKernel>) -> BTreeSet<String> {
    let all: Value = serde_json::from_str(
        &doc.query(&json!({"query": "parameters"}).to_string())
            .unwrap(),
    )
    .unwrap();
    all.as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_owned())
        .collect()
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

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

/// Whether the line `id` runs from `a` to `b`.
fn runs(s: &Value, id: &str, a: [f64; 2], b: [f64; 2]) -> bool {
    let g = &find(s, id).unwrap_or_else(|| panic!("no {id} in {s}"))["geometry"];
    near(xy(&g["start"]), a) && near(xy(&g["end"]), b)
}

fn shown_notes(result: &Value) -> Vec<String> {
    result["note_messages"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn notes(result: &Value) -> Vec<String> {
    result["notes"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn pattern<'a>(s: &'a Value, id: &str) -> Option<&'a Value> {
    s["patterns"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["id"] == id))
}

/// The copies of `original` the pattern `id` makes, by instance.
fn copies_of(s: &Value, id: &str, original: &str) -> Vec<String> {
    pattern(s, id).unwrap_or_else(|| panic!("no pattern {id} in {s}"))["copies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| self::id(&c["entities"][original]))
        .collect()
}

fn line(d: &mut Document<MockKernel>, start: Value, end: Value) -> String {
    id(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
    )["made"][0])
}

/// A pattern made by `cmd`: its id and the parameters it made.
fn patterned(d: &mut Document<MockKernel>, cmd: Value) -> (String, BTreeSet<String>) {
    let before = parameters(d);
    let made = command(d, cmd);
    let id = id(&made["constraints"][0]);
    let new: BTreeSet<String> = parameters(d).difference(&before).cloned().collect();
    (id, new)
}

/// A sketch with a line from (0, 0) to (10, 0) in a rectangular pattern
/// of three along x, 20 mm apart, and a line from (10, 50) to (20, 50) in
/// a circular pattern of three about (0, 50). The lines, the patterns'
/// ids and their parameters.
struct Two {
    doc: Document<MockKernel>,
    straight: String,
    rectangular: String,
    spacings: BTreeSet<String>,
    turned: String,
    circular: String,
    angle: BTreeSet<String>,
}

fn two_patterns() -> Two {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let straight = line(&mut doc, json!([0, 0]), json!([10, 0]));
    let (rectangular, spacings) = patterned(
        &mut doc,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": [straight],
               "direction": [1, 0], "count": [3, 1], "spacing": [20, 10]}),
    );
    let turned = line(&mut doc, json!([10, 50]), json!([20, 50]));
    let (circular, angle) = patterned(
        &mut doc,
        json!({"cmd": "sketch.circular_pattern", "sketch": "F1", "entities": [turned],
               "center": [0, 50], "count": 3}),
    );
    assert_eq!(spacings.len(), 2, "two spacing parameters: {spacings:?}");
    assert_eq!(angle.len(), 1, "one angle parameter: {angle:?}");
    Two {
        doc,
        straight,
        rectangular,
        spacings,
        turned,
        circular,
        angle,
    }
}

/// The originals where they were made.
fn originals_in_place(s: &Value, t: &Two) {
    assert!(runs(s, &t.straight, [0.0, 0.0], [10.0, 0.0]), "{s}");
    assert!(runs(s, &t.turned, [10.0, 50.0], [20.0, 50.0]), "{s}");
}

#[test]
fn deleting_every_copy_in_one_remove_takes_both_patterns_and_their_parameters() {
    let mut t = two_patterns();
    let s = sketch(&t.doc);
    let mut copies = copies_of(&s, &t.rectangular, &t.straight);
    copies.extend(copies_of(&s, &t.circular, &t.turned));
    assert_eq!(copies.len(), 4, "{s}");
    let removed = command(
        &mut t.doc,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": copies}),
    );
    // One note per pattern, in plain words and with its id for agents.
    let shown = shown_notes(&removed);
    assert_eq!(
        shown.iter().filter(|n| *n == REMOVED).count(),
        2,
        "{removed}"
    );
    let notes = notes(&removed);
    for p in [&t.rectangular, &t.circular] {
        let named = format!("pattern {p} was removed with its last copy");
        assert!(notes.contains(&named), "{removed}");
    }
    let s = sketch(&t.doc);
    assert!(pattern(&s, &t.rectangular).is_none(), "{s}");
    assert!(pattern(&s, &t.circular).is_none(), "{s}");
    for c in &copies {
        assert!(find(&s, c).is_none(), "{s}");
    }
    originals_in_place(&s, &t);
    let names = parameters(&t.doc);
    for p in t.spacings.iter().chain(&t.angle) {
        assert!(!names.contains(p), "{p} is still there: {names:?}");
    }

    // One undo brings both patterns, their copies and parameters back.
    command(&mut t.doc, json!({"cmd": "undo"}));
    let s = sketch(&t.doc);
    assert_eq!(copies_of(&s, &t.rectangular, &t.straight).len(), 2, "{s}");
    assert_eq!(copies_of(&s, &t.circular, &t.turned).len(), 2, "{s}");
    let names = parameters(&t.doc);
    for p in t.spacings.iter().chain(&t.angle) {
        assert!(names.contains(p), "{p} is not back: {names:?}");
    }
}

#[test]
fn deleting_the_copies_one_by_one_takes_the_pattern_with_the_last() {
    let mut t = two_patterns();
    for (id, original, params) in [
        (
            t.rectangular.clone(),
            t.straight.clone(),
            t.spacings.clone(),
        ),
        (t.circular.clone(), t.turned.clone(), t.angle.clone()),
    ] {
        let s = sketch(&t.doc);
        let copies = copies_of(&s, &id, &original);
        assert_eq!(copies.len(), 2, "{s}");
        // Deleting some copies keeps the pattern, the instance skipped.
        let first = command(
            &mut t.doc,
            json!({"cmd": "sketch.remove", "sketch": "F1", "items": [copies[0]]}),
        );
        assert!(!shown_notes(&first).iter().any(|n| n == REMOVED), "{first}");
        let s = sketch(&t.doc);
        let kept = pattern(&s, &id).unwrap_or_else(|| panic!("no {id} in {s}"));
        assert_eq!(kept["skipped"], json!([[1, 0]]), "{s}");
        assert!(params.is_subset(&parameters(&t.doc)));
        // The last copy takes it.
        let last = command(
            &mut t.doc,
            json!({"cmd": "sketch.remove", "sketch": "F1", "items": [copies[1]]}),
        );
        assert_eq!(shown_notes(&last), [REMOVED], "{last}");
        let s = sketch(&t.doc);
        assert!(pattern(&s, &id).is_none(), "{s}");
        let names = parameters(&t.doc);
        assert!(params.is_disjoint(&names), "{params:?} in {names:?}");
        originals_in_place(&s, &t);

        // One undo brings the pattern and its parameters back, with the
        // first copy still left out.
        command(&mut t.doc, json!({"cmd": "undo"}));
        let s = sketch(&t.doc);
        let back = pattern(&s, &id).unwrap_or_else(|| panic!("no {id} in {s}"));
        assert_eq!(back["skipped"], json!([[1, 0]]), "{s}");
        assert_eq!(copies_of(&s, &id, &original), [copies[1].clone()], "{s}");
        assert!(params.is_subset(&parameters(&t.doc)));
        // And again for the next pattern's run.
        command(
            &mut t.doc,
            json!({"cmd": "sketch.remove", "sketch": "F1", "items": [copies[1]]}),
        );
    }
    let s = sketch(&t.doc);
    assert!(s["patterns"].as_array().is_none_or(Vec::is_empty), "{s}");
    originals_in_place(&s, &t);
}

/// The design saved with the rectangular pattern's record kept with every
/// instance skipped and no copy, as 0.66.0 saved it; the circular pattern
/// stays whole. A user parameter `gap` uses the first spacing.
fn saved_with_an_emptied_record(t: &mut Two) -> String {
    let first = t.spacings.first().unwrap().clone();
    command(
        &mut t.doc,
        json!({"cmd": "add_parameter", "name": "gap", "expression": format!("{first} * 2")}),
    );
    let mut saved: Value = serde_json::from_str(&t.doc.to_json()).unwrap();
    let record = record_mut(&mut saved, &t.rectangular);
    record["copies"] = json!([]);
    record["skipped"] = json!([[1, 0], [2, 0]]);
    saved.to_string()
}

/// The pattern record `id` of the sketch F1 in a design file.
fn record_mut<'a>(design: &'a mut Value, id: &str) -> &'a mut Value {
    let feature = design["features"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["uid"] == "F1")
        .unwrap();
    feature["patterns"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == id)
        .unwrap_or_else(|| panic!("no pattern {id} in the file"))
}

/// The pattern record `id` as the design file `json` stores it.
fn saved_record(json: &str, id: &str) -> Value {
    let mut design: Value = serde_json::from_str(json).unwrap();
    record_mut(&mut design, id).clone()
}

#[test]
fn an_emptied_record_of_a_saved_design_goes_with_the_next_edit_of_its_sketch() {
    let mut t = two_patterns();
    let saved = saved_with_an_emptied_record(&mut t);
    let used = t.spacings.first().unwrap().clone();
    let free = t.spacings.last().unwrap().clone();
    let mut d = Document::from_json(&saved, MockKernel::default()).unwrap();
    d.recompute();
    // Opened, nothing changes: the record and its parameters are there.
    let s = sketch(&d);
    let record = pattern(&s, &t.rectangular).unwrap_or_else(|| panic!("{s}"));
    assert_eq!(record["skipped"], json!([[1, 0], [2, 0]]), "{s}");
    assert!(t.spacings.is_subset(&parameters(&d)));
    assert_eq!(
        saved_record(&d.to_json(), &t.rectangular),
        saved_record(&saved, &t.rectangular)
    );

    // The next edit of the sketch removes it, with the note.
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [100, 100]}),
    );
    assert_eq!(shown_notes(&added), [REMOVED], "{added}");
    let s = sketch(&d);
    assert!(pattern(&s, &t.rectangular).is_none(), "{s}");
    // The other pattern is unchanged.
    assert_eq!(copies_of(&s, &t.circular, &t.turned).len(), 2, "{s}");
    originals_in_place(&s, &t);
    let names = parameters(&d);
    // The spacing `gap` uses stays; the other goes; the angle stays.
    assert!(names.contains(&used), "{names:?}");
    assert!(!names.contains(&free), "{names:?}");
    assert!(t.angle.is_subset(&names), "{names:?}");
    assert!(names.contains("gap"), "{names:?}");

    // One undo brings the record back as it was saved.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    let record = pattern(&s, &t.rectangular).unwrap_or_else(|| panic!("{s}"));
    assert_eq!(record["skipped"], json!([[1, 0], [2, 0]]), "{s}");
    assert!(t.spacings.is_subset(&parameters(&d)));
}
