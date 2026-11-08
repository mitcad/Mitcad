// SPDX-License-Identifier: MIT
//! Lengths entered in a design in inches (mitcad#1452, GitHub
//! mitcad/Mitcad#187): every value text a sketch or a feature takes reads
//! its bare numbers in the design's unit, as the `evaluate` query that the
//! application's value fields show reads them, also for a dimension made
//! before the design's unit changed; JSON numbers stay millimetres. The
//! application's typed values are checked by
//! `tools/ui-sketch-inch-entry-test.sh`.

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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// The parameter `name` as the `parameters` query gives it.
fn parameter(doc: &Document<MockKernel>, name: &str) -> Value {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .cloned()
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

/// Parameter `name`'s value in millimetres, to 1e-9 mm.
fn assert_mm(doc: &Document<MockKernel>, name: &str, mm: f64) {
    let p = parameter(doc, name);
    let value = p["value"].as_f64().unwrap();
    assert!(
        (value - mm).abs() < 1e-9,
        "{name}: {value} mm, not {mm} mm: {p}"
    );
}

#[test]
fn typed_lengths_in_an_inch_design_are_inches() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "set_units", "length": "in"}));
    command(&mut doc, json!({"cmd": "sketch.create"}));
    // The reported bar: 0.875 x 12.125 in and a 0.2 in hole, the values
    // as the application sends what is typed (a text).
    let bar = command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": "0.875", "height": "12.125"}),
    );
    assert_eq!(bar["parameters"], json!(["d1", "d2"]), "{bar}");
    assert_mm(&doc, "d1", 22.225);
    assert_mm(&doc, "d2", 307.975);
    assert_eq!(parameter(&doc, "d1")["unit"], "in");
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [11.1125, 282.575],
               "diameter": "0.2 in"}),
    );
    assert_mm(&doc, "d3", 5.08);
    // The value fields' preview: the same number, shown in inches.
    let typed = query(&doc, json!({"query": "evaluate", "expression": "0.875"}));
    assert!((typed["value"].as_f64().unwrap() - 22.225).abs() < 1e-9);
    assert_eq!(typed["text"], "0.875 in");
    // JSON numbers are millimetres in every design (commands.md, Values):
    // 0.875 as a number is 0.875 mm, written in inches.
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [100, 0],
               "width": 0.875, "height": 25.4}),
    );
    assert_mm(&doc, "d4", 0.875);
    assert_mm(&doc, "d5", 25.4);
    assert_eq!(parameter(&doc, "d5")["expression"], "1 in");
}

#[test]
fn a_dimension_made_before_the_unit_change_reads_typed_numbers_in_the_new_unit() {
    // A sketch dimensioned while the design was in millimetres, then the
    // design set to inches: its dimension's parameter is still in mm.
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let circle = command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10}),
    );
    let dimension = circle["dimensions"][0].as_str().unwrap().to_owned();
    assert_eq!(parameter(&doc, "d1")["unit"], "mm");
    command(&mut doc, json!({"cmd": "set_units", "length": "in"}));
    assert_mm(&doc, "d1", 10.0);
    // An expression typed into its value editor: the editor's preview reads
    // it in inches (12.7 mm), and so does the dimension, which takes the
    // design's unit (before: 0.5 mm).
    let preview = query(&doc, json!({"query": "evaluate", "expression": "0.25 * 2"}));
    assert!((preview["value"].as_f64().unwrap() - 12.7).abs() < 1e-9);
    command(
        &mut doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension,
               "value": "0.25 * 2"}),
    );
    assert_mm(&doc, "d1", 12.7);
    assert_eq!(parameter(&doc, "d1")["unit"], "in");
    // A bare number, and one with its own unit.
    command(
        &mut doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension,
               "value": "0.875"}),
    );
    assert_mm(&doc, "d1", 22.225);
    command(
        &mut doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension,
               "value": "10 mm"}),
    );
    assert_mm(&doc, "d1", 10.0);
    // `name = value` names the dimension's parameter as it sets it.
    command(
        &mut doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension,
               "value": "hole = 0.1"}),
    );
    assert_mm(&doc, "hole", 2.54);
    // A user parameter the dimension names keeps its own unit.
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "w", "expression": "3", "unit": "mm"}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension,
               "value": "w"}),
    );
    assert_mm(&doc, "w", 3.0);
    assert_eq!(parameter(&doc, "w")["unit"], "mm");
}

#[test]
fn a_feature_value_made_before_the_unit_change_reads_typed_numbers_in_the_new_unit() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    let rectangle = command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    );
    let region = rectangle["region"].clone();
    let extrude = |distance: Value| {
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": distance}, "operation": "new_body"})
    };
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": extrude(json!(20))}),
    );
    // Its distance's own parameter (`Extrude1 distance`).
    let distance = query(&doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| {
            p["comment"]
                .as_str()
                .is_some_and(|c| c.ends_with("distance"))
        })
        .and_then(|p| p["name"].as_str())
        .expect("the extrude's distance parameter")
        .to_owned();
    assert_eq!(parameter(&doc, &distance)["unit"], "mm");
    command(&mut doc, json!({"cmd": "set_units", "length": "in"}));
    // The command panel's expression: 0.5 * 2 in, as its preview read it.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": extrude(json!("0.5 * 2"))}),
    );
    assert_mm(&doc, &distance, 25.4);
    assert_eq!(parameter(&doc, &distance)["unit"], "in");
    // A JSON number is still millimetres.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": extrude(json!(15))}),
    );
    assert_mm(&doc, &distance, 15.0);
}
