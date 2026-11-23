// SPDX-License-Identifier: MIT
//! Equation Curve (mitcad#2237) through the JSON api: a curve of formulas
//! of the design's parameters that follows a changed parameter at the
//! next recompute, renames with it, keeps a joined line joined across an
//! edit, and is one undo step.

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

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(answer) => panic!("{command} was not refused: {answer}"),
        Err(e) => e.to_string(),
    }
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn entity(s: &Value, id: &str) -> Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

fn involute(r: f64, t: f64) -> [f64; 2] {
    [r * (t.cos() + t * t.sin()), r * (t.sin() - t * t.cos())]
}

fn design() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "xy"}));
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "r", "expression": "5 mm"}),
    );
    doc
}

#[test]
fn involute_follows_and_renames_its_parameter() {
    let mut doc = design();
    let made = command(
        &mut doc,
        json!({"cmd": "sketch.equation_curve", "sketch": "F1", "mode": "parametric",
               "x": "r*(cos(t)+t*sin(t))", "y": "r*(sin(t)-t*cos(t))", "start": 0, "end": 2}),
    );
    let curve = made["curve"].as_str().unwrap().to_owned();
    assert_eq!(made["made"], json!([curve]));
    let count = made["point_count"].as_u64().unwrap() as usize;
    assert!((8..=200).contains(&count), "{made}");
    assert_eq!(made["points"].as_array().unwrap().len(), count);
    assert_eq!(made["closed"], false);
    assert!(made["deviation"].as_f64().unwrap() <= 0.01);
    assert_eq!(
        made["step"].as_str().map(|s| s.starts_with('s')),
        Some(true)
    );
    let end = made["points"][count - 1].as_str().unwrap().to_owned();
    let s = sketch(&doc);
    assert!(near(xy(&entity(&s, &end)["at"]), involute(5.0, 2.0)));
    assert_eq!(entity(&s, &end)["fixed"], true);
    assert_eq!(entity(&s, &curve)["type"], "fitted_spline");
    let q = &s["equations"][0];
    assert_eq!(q["curve"], curve.as_str());
    assert_eq!(q["mode"], "parametric");
    assert_eq!(q["start"], "0");
    assert_eq!(q["variable"], "t");
    assert_eq!(q["point_count"], count);
    // r changes: the curve follows at the recompute, its points as many.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "r", "expression": "8 mm"}),
    );
    let s = sketch(&doc);
    assert!(near(xy(&entity(&s, &end)["at"]), involute(8.0, 2.0)), "{s}");
    assert_eq!(s["equations"][0]["point_count"], count);
    // A rename rewrites the formulas.
    command(
        &mut doc,
        json!({"cmd": "rename_parameter", "name": "r", "new_name": "base_radius"}),
    );
    let s = sketch(&doc);
    assert_eq!(s["equations"][0]["x"], "base_radius*(cos(t)+t*sin(t))");
    assert_eq!(s["equations"][0]["y"], "base_radius*(sin(t)-t*cos(t))");
    assert!(near(xy(&entity(&s, &end)["at"]), involute(8.0, 2.0)));
}

#[test]
fn edit_keeps_a_joined_line_and_is_one_undo_step() {
    let mut doc = design();
    let made = command(
        &mut doc,
        json!({"cmd": "sketch.equation_curve", "sketch": "F1", "y": "x^2/20",
               "start": 0, "end": "10"}),
    );
    let curve = made["curve"].as_str().unwrap().to_owned();
    let points = made["points"].as_array().unwrap().clone();
    let (first, last) = (points[0].clone(), points[points.len() - 1].clone());
    let line = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": last, "end": [20, 0]}),
    );
    let line = line["made"][0].as_str().unwrap().to_owned();
    let edited = command(
        &mut doc,
        json!({"cmd": "sketch.edit_equation_curve", "sketch": "F1", "curve": curve,
               "y": "x^2/10"}),
    );
    assert_eq!(edited["points"][0], first);
    let n = edited["point_count"].as_u64().unwrap() as usize;
    assert_eq!(edited["points"][n - 1], last);
    let s = sketch(&doc);
    assert_eq!(s["equations"][0]["y"], "x^2/10");
    assert_eq!(s["equations"][0]["start"], "0 mm");
    assert_eq!(entity(&s, &line)["start"], last);
    let start = xy(&entity(&s, &line)["geometry"]["start"]);
    assert!(near(start, [10.0, 10.0]), "{start:?}");
    // One undo step: the formula before.
    command(&mut doc, json!({"cmd": "undo"}));
    let s = sketch(&doc);
    assert_eq!(s["equations"][0]["y"], "x^2/20");
    let start = xy(&entity(&s, &line)["geometry"]["start"]);
    assert!(near(start, [10.0, 5.0]), "{start:?}");
    // The creation is one step too.
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    let s = sketch(&doc);
    assert!(s.get("equations").is_none_or(Value::is_null), "{s}");
}

#[test]
fn refusals_name_the_problem() {
    let mut doc = design();
    let message = refused(
        &mut doc,
        json!({"cmd": "sketch.equation_curve", "sketch": "F1", "y": "x * k",
               "start": 0, "end": 1}),
    );
    assert!(message.contains("k is not a parameter"), "{message}");
    let message = refused(
        &mut doc,
        json!({"cmd": "sketch.equation_curve", "sketch": "F1", "mode": "polar",
               "r": "10", "start": 90, "end": 0}),
    );
    assert!(message.contains("empty or reversed"), "{message}");
    let message = refused(
        &mut doc,
        json!({"cmd": "sketch.equation_curve", "sketch": "F1", "y": "ln(x)",
               "start": -1, "end": 1}),
    );
    assert!(message.contains("cannot be evaluated"), "{message}");
    let message = refused(
        &mut doc,
        json!({"cmd": "sketch.edit_equation_curve", "sketch": "F1", "curve": "c9", "y": "x"}),
    );
    assert!(message.contains("c9"), "{message}");
    // Nothing was added.
    let s = sketch(&doc);
    assert!(s.get("equations").is_none_or(Value::is_null), "{s}");
    assert!(
        !s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "fitted_spline")
    );
}
