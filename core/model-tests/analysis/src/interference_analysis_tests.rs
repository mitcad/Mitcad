// SPDX-License-Identifier: MIT
//! Interference checks kept in the document (mitcad#2664) on the mock
//! kernel (`MockKernel::box_interferences`: the overlap of the bodies'
//! boxes): a check of two of three bodies kept as `Interference1` and run
//! again by its name after the geometry changes (the overlap gone and back),
//! a check of every body that takes in a body added after it was kept, a
//! check never shown that leaves the shown section shown, a body that is
//! gone said in words until the check is edited or an undo brings it back,
//! refused definitions, the checks saved and read back, and the model left
//! as it was by keeping and running them.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn query_error(doc: &Document<MockKernel>, query: Value) -> String {
    match doc.query(&query.to_string()) {
        Ok(answer) => panic!("{query} was answered: {answer}"),
        Err(e) => e.to_string(),
    }
}

fn analyses(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "analyses"}))
        .as_array()
        .unwrap()
        .clone()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

/// A 10 mm square on `sketch` at `corner`, extruded 10 mm from `start`
/// (mm or a parameter's name) into a new body.
fn block(sketch: &str, corner: [f64; 2], start: Value) -> Value {
    json!([
        {"cmd": "sketch.create"},
        {"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": corner, "width": 10, "height": 10},
        {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": sketch, "region": R}],
            "start": {"type": "offset", "offset": start},
            "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
    ])
}

/// Three bodies on XY: Body1 (F2.b0) at the origin, Body2 (F4.b0) 5 mm
/// along X, raised by the parameter `lift` (0 mm), so that the two share
/// a 5 x 10 x 10 mm box (500 mm³), and Body3 (F6.b0) at x 40, apart.
fn design() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_interferences.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "lift", "value": "0 mm"}),
    );
    command(&mut doc, block("F1", [0.0, 0.0], json!(0)));
    command(&mut doc, block("F3", [5.0, 0.0], json!("lift")));
    command(&mut doc, block("F5", [40.0, 0.0], json!(0)));
    doc
}

/// Body4 (F8.b0), 5 mm along X from Body3: they share 500 mm³.
fn add_body4(doc: &mut Document<MockKernel>) {
    command(doc, block("F7", [45.0, 0.0], json!(0)));
}

fn two_bodies() -> Value {
    json!({"type": "interference", "bodies": ["F2.b0", "F4.b0"]})
}

/// The kept check's pairs as (a, b, volume).
fn pairs(answer: &Value) -> Vec<(String, String, f64)> {
    answer["pairs"]
        .as_array()
        .unwrap_or_else(|| panic!("no pairs: {answer}"))
        .iter()
        .map(|pair| {
            (
                pair["a"].as_str().unwrap().to_owned(),
                pair["b"].as_str().unwrap().to_owned(),
                pair["volume"].as_f64().unwrap(),
            )
        })
        .collect()
}

fn run(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "interference", "name": name}))
}

fn assert_pairs(answer: &Value, expected: &[(&str, &str, f64)]) {
    let found = pairs(answer);
    assert_eq!(found.len(), expected.len(), "{answer}");
    for ((a, b, volume), (ea, eb, ev)) in found.iter().zip(expected) {
        assert_eq!((a.as_str(), b.as_str()), (*ea, *eb), "{answer}");
        assert!((volume - ev).abs() < 1e-6, "{volume} instead of {ev}");
    }
}

#[test]
fn a_kept_check_runs_again_on_its_bodies_after_the_geometry_changes() {
    let mut doc = design();
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": two_bodies()}),
    );
    assert_eq!(added["name"], "Interference1");
    assert_eq!(undo_label(&doc), "Add Interference1");
    let listed = analyses(&doc);
    assert_eq!(listed[0]["type"], "interference");
    assert_eq!(listed[0]["bodies"], json!(["F2.b0", "F4.b0"]));
    assert_eq!(listed[0]["visible"], false);
    assert!(listed[0].get("error").is_none(), "{}", listed[0]);

    let first = run(&doc, "Interference1");
    assert_eq!(first["bodies"], 2);
    assert_pairs(&first, &[("F2.b0", "F4.b0", 500.0)]);
    // The same as the query on the same bodies.
    let direct = query(
        &doc,
        json!({"query": "interference", "bodies": ["F2.b0", "F4.b0"]}),
    );
    assert_eq!(first, direct);

    // Body2 raised clear of Body1: the overlap is gone.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "lift", "value": 15}),
    );
    let clear = run(&doc, "Interference1");
    assert_eq!(clear["bodies"], 2);
    assert_pairs(&clear, &[]);
    // Half way down: half the overlap.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "lift", "value": 5}),
    );
    assert_pairs(&run(&doc, "Interference1"), &[("F2.b0", "F4.b0", 250.0)]);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "lift", "value": 0}),
    );

    // A body added since: not in this check's scope.
    add_body4(&mut doc);
    let kept = run(&doc, "Interference1");
    assert_eq!(kept["bodies"], 2);
    assert_pairs(&kept, &[("F2.b0", "F4.b0", 500.0)]);
}

#[test]
fn a_check_of_every_body_takes_in_bodies_added_later() {
    let mut doc = design();
    let added = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "interference"}}),
    );
    assert_eq!(added["name"], "Interference1");
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(
        saved["analyses"],
        json!([{"name": "Interference1", "visible": false, "type": "interference"}])
    );
    let before = run(&doc, "Interference1");
    assert_eq!(before["bodies"], 3);
    assert_pairs(&before, &[("F2.b0", "F4.b0", 500.0)]);
    add_body4(&mut doc);
    let after = run(&doc, "Interference1");
    assert_eq!(after["bodies"], 4);
    assert_pairs(
        &after,
        &[("F2.b0", "F4.b0", 500.0), ("F6.b0", "F8.b0", 500.0)],
    );
    // Body2 raised clear: Body3 and Body4 still overlap.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "lift", "value": 15}),
    );
    assert_pairs(&run(&doc, "Interference1"), &[("F6.b0", "F8.b0", 500.0)]);
}

#[test]
fn a_check_is_never_shown_and_leaves_the_shown_analysis_shown() {
    let mut doc = design();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xz"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": two_bodies()}),
    );
    let shown = |doc: &Document<MockKernel>| -> Vec<bool> {
        analyses(doc)
            .iter()
            .map(|a| a["visible"].as_bool().unwrap())
            .collect()
    };
    assert_eq!(shown(&doc), [true, false]);
    // Showing it is refused in words; hiding it changes nothing.
    let error = refused(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Interference1", "visible": true}),
    );
    assert!(error.contains("not shown in the view"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "set_analysis_visible", "name": "Interference1", "visible": false}),
    );
    assert_eq!(undo_label(&doc), "Add Interference1");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Interference1", "def": two_bodies(), "visible": true}),
    );
    assert!(error.contains("not shown in the view"), "{error}");
    // Edited with other bodies: the section stays shown.
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Interference1",
               "def": {"type": "interference", "bodies": ["F2.b0", "F6.b0"]}}),
    );
    assert_eq!(undo_label(&doc), "Edit Interference1");
    assert_eq!(shown(&doc), [true, false]);
    assert_pairs(&run(&doc, "Interference1"), &[]);
    // The same definition again is no undo step.
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Interference1",
               "def": {"type": "interference", "bodies": ["F2.b0", "F6.b0"]}}),
    );
    assert_eq!(undo_label(&doc), "Edit Interference1");
    // A second check is Interference2; the section is still the shown one.
    let second = command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "interference"}}),
    );
    assert_eq!(second["name"], "Interference2");
    assert_eq!(shown(&doc), [true, false, false]);

    // A file that leaves a check's `visible` out, the check before the
    // shown section: the section stays shown, the check is not.
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let analyses_in_file = file["analyses"].as_array_mut().unwrap();
    let section = analyses_in_file.remove(0);
    analyses_in_file.push(section);
    for analysis in analyses_in_file.iter_mut() {
        analysis.as_object_mut().unwrap().remove("visible");
    }
    let loaded = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    let shown_loaded: Vec<(String, bool)> = analyses(&loaded)
        .iter()
        .map(|a| {
            (
                a["name"].as_str().unwrap().to_owned(),
                a["visible"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        shown_loaded,
        [
            ("Interference1".to_owned(), false),
            ("Interference2".to_owned(), false),
            ("Section1".to_owned(), true),
        ]
    );
}

#[test]
fn a_body_that_is_gone_is_said_until_the_check_is_edited() {
    let mut doc = design();
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": two_bodies()}),
    );
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F4"}));
    let listed = analyses(&doc);
    let error = listed[0]["error"].as_str().unwrap_or_default().to_owned();
    assert!(
        error.contains("one of its bodies is not in the design at the timeline marker"),
        "{}",
        listed[0]
    );
    // In words: no body's id or name.
    assert!(!error.contains("F4") && !error.contains("Body2"), "{error}");
    // Run by its name: why it cannot run.
    let not_run = query_error(
        &doc,
        json!({"query": "interference", "name": "Interference1"}),
    );
    assert!(not_run.contains("one of its bodies"), "{not_run}");
    // An undo brings the body back, and the check runs again.
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(analyses(&doc)[0].get("error").is_none());
    assert_pairs(&run(&doc, "Interference1"), &[("F2.b0", "F4.b0", 500.0)]);
    // Gone again, the check edited to other bodies: it runs on them.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F4"}));
    command(
        &mut doc,
        json!({"cmd": "edit_analysis", "name": "Interference1",
               "def": {"type": "interference", "bodies": ["F2.b0", "F6.b0"]}}),
    );
    assert!(analyses(&doc)[0].get("error").is_none());
    let edited = run(&doc, "Interference1");
    assert_eq!(edited["bodies"], 2);
    assert_pairs(&edited, &[]);
    // Both of its bodies gone: said with their count.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F6"}));
    let two_gone = analyses(&doc)[0]["error"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        two_gone.contains("2 of its bodies are not in the design"),
        "{two_gone}"
    );
}

#[test]
fn definitions_are_refused_with_the_reason() {
    let mut doc = design();
    for (def, expected) in [
        (
            json!({"type": "interference", "bodies": ["F2.b0", "F2.b0"]}),
            "lists a body more than once",
        ),
        (
            json!({"type": "interference", "bodies": ["F2.b0", "F9.b0"]}),
            "one of its bodies is not in the design at the timeline marker",
        ),
        (
            json!({"type": "interference", "bodies": ["F2.b0"], "min_volume": 1}),
            "min_volume",
        ),
    ] {
        let error = refused(&mut doc, json!({"cmd": "add_analysis", "def": def.clone()}));
        assert!(error.contains(expected), "{def}: {error}");
    }
    assert!(analyses(&doc).is_empty());
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "section", "plane": "xy"}}),
    );
    // The query wants a kept check's name or the bodies, not both; a
    // section is not a check.
    let both = query_error(
        &doc,
        json!({"query": "interference", "name": "Section1", "bodies": ["F2.b0"]}),
    );
    assert!(both.contains("not both"), "{both}");
    let section = query_error(&doc, json!({"query": "interference", "name": "Section1"}));
    assert!(
        section.contains("is not an interference check"),
        "{section}"
    );
    let unknown = query_error(&doc, json!({"query": "interference", "name": "Check7"}));
    assert!(unknown.contains("there is no analysis Check7"), "{unknown}");
}

#[test]
fn checks_are_saved_and_read_back_and_leave_the_model_as_it_was() {
    let mut doc = design();
    let bodies_before = query(&doc, json!({"query": "bodies"}));
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": two_bodies()}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_analysis", "def": {"type": "interference"}}),
    );
    // Keeping them changed no body.
    assert_eq!(query(&doc, json!({"query": "bodies"})), bodies_before);
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["analyses"],
        json!([
            {"name": "Interference1", "visible": false, "type": "interference",
             "bodies": ["F2.b0", "F4.b0"]},
            {"name": "Interference2", "visible": false, "type": "interference"}
        ])
    );
    // Running them changes nothing: no body, no undo step, no file change.
    let label = undo_label(&doc);
    run(&doc, "Interference1");
    run(&doc, "Interference2");
    assert_eq!(doc.to_json(), saved);
    assert_eq!(undo_label(&doc), label);
    assert_eq!(query(&doc, json!({"query": "bodies"})), bodies_before);

    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.kernel().box_interferences.set(true);
    assert_eq!(loaded.to_json(), saved);
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(analyses(&loaded), analyses(&doc));
    assert_pairs(&run(&loaded, "Interference1"), &[("F2.b0", "F4.b0", 500.0)]);
    let all = run(&loaded, "Interference2");
    assert_eq!(all["bodies"], 3);
    assert_pairs(&all, &[("F2.b0", "F4.b0", 500.0)]);
    // Renamed, the check runs by its new name.
    command(
        &mut loaded,
        json!({"cmd": "rename_analysis", "name": "Interference1", "new_name": "Lid and base"}),
    );
    assert_pairs(&run(&loaded, "Lid and base"), &[("F2.b0", "F4.b0", 500.0)]);
}
