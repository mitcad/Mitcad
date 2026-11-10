// SPDX-License-Identifier: MIT
//! Inactive sketch constraints and dimensions (mitcad#721): the solve
//! leaves them out, `sketch.set_active` activates them only where the
//! sketch then solves, the file keeps them, and undo takes the switch back.

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

fn first(result: &Value, list: &str, prefix: char) -> String {
    result[list]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| id.starts_with(prefix))
        .unwrap_or_else(|| panic!("no {prefix}… in {result}"))
        .to_owned()
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

/// The `constraints` or `dimensions` entry of the sketch query with this id.
fn entry(s: &Value, list: &str, id: &str) -> Option<Value> {
    s[list]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == id)
        .cloned()
}

/// The first feature (the sketch) of a saved design, as text.
fn sketch_feature(file: &str) -> String {
    let value: Value = serde_json::from_str(file).unwrap();
    value["features"][0].to_string()
}

fn set_active(doc: &mut Document<MockKernel>, items: &[&str], active: bool) -> Value {
    command(
        doc,
        json!({"cmd": "sketch.set_active", "sketch": "F1", "items": items, "active": active}),
    )
}

/// A sketch with a line from the fixed p1 at (0, 0) to (10, 5): the line
/// and its end point.
fn sloped_line() -> (Document<MockKernel>, String, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p1", "end": [10, 5]}),
    );
    let line = first(&made, "entities", 'c');
    let end = first(&made, "entities", 'p');
    (d, line, end)
}

#[test]
fn the_solve_leaves_an_inactive_constraint_out_and_activation_is_checked() {
    let (mut d, line, end) = sloped_line();
    // Its length kept, so that turning it never collapses it; it is turned
    // from a slope each time, never across the point it turns about.
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": 125f64.sqrt(),
               "dimension": {"type": "length", "line": line}}),
    );
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": line}}),
    );
    let horizontal = first(&added, "constraints", 'k');
    let [_, y] = at(&sketch(&d), &end);
    assert!(y.abs() < 1e-9, "horizontal: {y}");

    // Deactivated, it constrains nothing: a vertical constraint is taken.
    let result = set_active(&mut d, &[horizontal.as_str()], false);
    assert_eq!(result["made"], json!([horizontal]), "{result}");
    assert_eq!(d.undo_label(), Some("Deactivate in Sketch1"));
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": end, "to": [5, 10]}),
    );
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "vertical", "line": line}}),
    );
    let vertical = first(&added, "constraints", 'k');
    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    let [x, _] = at(&s, &end);
    assert!(x.abs() < 1e-9, "vertical: {x}");
    let k = entry(&s, "constraints", &horizontal).unwrap();
    assert_eq!(k["active"], false, "{k}");
    // The geometry does not meet it now: information, not a problem
    // (mitcad#1040).
    assert_eq!(s["unmet_inactive"], json!([horizontal]), "{s}");
    assert!(
        s["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["level"] == "info"
                && n["message"] == "An inactive constraint the geometry does not meet"),
        "{s}"
    );

    // Activating it would contradict the vertical one: refused, nothing
    // changes.
    let depth = d.undo_depth();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.set_active", "sketch": "F1", "items": [horizontal]}),
    );
    assert!(error.contains("cannot be activated"), "{error}");
    assert_eq!(d.undo_depth(), depth);
    assert_eq!(
        entry(&sketch(&d), "constraints", &horizontal).unwrap()["active"],
        false
    );

    // Without the vertical one it is activated, and the line is horizontal
    // again.
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [vertical]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": end, "to": [8, 8]}),
    );
    set_active(&mut d, &[horizontal.as_str()], true);
    let s = sketch(&d);
    let k = entry(&s, "constraints", &horizontal).unwrap();
    assert!(k.get("active").is_none(), "an active one has no flag: {k}");
    assert!(at(&s, &end)[1].abs() < 1e-9, "{s}");
    assert!(s.get("unmet_inactive").is_none(), "{s}");

    // Undo takes the activation back; delete removes an inactive one.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert_eq!(
        entry(&s, "constraints", &horizontal).unwrap()["active"],
        false
    );
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [horizontal]}),
    );
    assert!(entry(&sketch(&d), "constraints", &horizontal).is_none());
}

#[test]
fn an_inactive_dimension_keeps_its_value_but_drives_nothing() {
    let (mut d, line, end) = sloped_line();
    // The line is as long as it is drawn, sqrt(125) mm.
    let length = 125f64.sqrt();
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": length,
               "dimension": {"type": "length", "line": line}}),
    );
    let dimension = first(&added, "dimensions", 'k');
    let len = |s: &Value| {
        let [x, y] = at(s, &end);
        x.hypot(y)
    };
    // Deactivated with a reason, then its value changed: the line keeps
    // its length.
    command(
        &mut d,
        json!({"cmd": "sketch.set_active", "sketch": "F1", "items": [dimension],
               "active": false, "reason": "not met in the imported file"}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": dimension,
               "value": 30}),
    );
    let s = sketch(&d);
    assert!((len(&s) - length).abs() < 1e-9, "{s}");
    let k = entry(&s, "dimensions", &dimension).unwrap();
    assert_eq!(
        (
            k["active"].clone(),
            k["inactive_reason"].clone(),
            k["measured"].clone()
        ),
        (
            json!(false),
            json!("not met in the imported file"),
            json!(30.0)
        ),
        "{k}"
    );
    assert_eq!(s["unmet_inactive"], json!([dimension]), "{s}");
    // Activated, it drives the line to 30 mm; the reason goes with the
    // flag.
    set_active(&mut d, &[dimension.as_str()], true);
    let s = sketch(&d);
    assert!((len(&s) - 30.0).abs() < 1e-9, "{s}");
    assert!(
        entry(&s, "dimensions", &dimension)
            .unwrap()
            .get("inactive_reason")
            .is_none()
    );
    command(&mut d, json!({"cmd": "undo"}));
    assert!((len(&sketch(&d)) - length).abs() < 1e-9);
}

#[test]
fn inactive_items_round_trip_through_the_file() {
    let (mut d, line, _) = sloped_line();
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": line}}),
    );
    let horizontal = first(&added, "constraints", 'k');
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": 10,
               "dimension": {"type": "length", "line": line}}),
    );
    let length = first(&added, "dimensions", 'k');
    let plain = d.to_json();
    command(
        &mut d,
        json!({"cmd": "sketch.set_active", "sketch": "F1", "items": [horizontal, length],
               "active": false, "reason": "not met in the imported file"}),
    );
    let json = d.to_json();
    let value: Value = serde_json::from_str(&json).unwrap();
    let feature = &value["features"][0];
    for (list, id) in [("constraints", &horizontal), ("dimensions", &length)] {
        let item = feature[list]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == *id)
            .unwrap();
        assert_eq!(item["active"], false, "{item}");
        assert_eq!(
            item["inactive_reason"], "not met in the imported file",
            "{item}"
        );
    }
    // Active items are saved as before: no flag.
    assert!(!sketch_feature(&plain).contains("\"active\""), "{plain}");
    let mut loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json, "saving again gives the same file");
    loaded.recompute();
    assert_eq!(sketch(&loaded), sketch(&d));
    // The loaded ones can be activated.
    set_active(&mut loaded, &[horizontal.as_str(), length.as_str()], true);
    let s = sketch(&loaded);
    assert!(s["error"].is_null(), "{s}");
    assert!(!sketch_feature(&loaded.to_json()).contains("\"active\""));
}

// --- Follow-ups (mitcad#723)

fn new_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn add_line(doc: &mut Document<MockKernel>, start: Value, end: Value) -> String {
    let made = command(
        doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
    );
    first(&made, "entities", 'c')
}

fn add_constraint(doc: &mut Document<MockKernel>, constraint: Value) -> Value {
    command(
        doc,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": constraint}),
    )
}

fn add_dimension(doc: &mut Document<MockKernel>, value: f64, dimension: Value) -> String {
    let added = command(
        doc,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1", "value": value,
               "dimension": dimension}),
    );
    first(&added, "dimensions", 'k')
}

fn set_dimension(doc: &mut Document<MockKernel>, id: &str, value: f64) {
    command(
        doc,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": id, "value": value}),
    );
}

/// A line's end point, as the sketch query names it.
fn end_of(s: &Value, line: &str) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == line)
        .and_then(|e| e["end"].as_str())
        .unwrap_or_else(|| panic!("no {line} in {s}"))
        .to_owned()
}

fn measured(s: &Value, id: &str) -> f64 {
    entry(s, "dimensions", id).unwrap_or_else(|| panic!("no {id} in {s}"))["measured"]
        .as_f64()
        .unwrap_or_else(|| panic!("{id} measures nothing: {s}"))
}

fn parameter_value(doc: &Document<MockKernel>, name: &str) -> f64 {
    let all: Value = serde_json::from_str(
        &doc.query(&json!({"query": "parameters"}).to_string())
            .unwrap(),
    )
    .unwrap();
    all.as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no parameter {name}"))["value"]
        .as_f64()
        .unwrap()
}

#[test]
fn a_point_merge_keeps_the_active_one_of_two_same_constraints() {
    let mut d = new_sketch();
    let a = add_line(&mut d, json!([0, 0]), json!([10, 0]));
    let b = add_line(&mut d, json!([20, 5]), json!([10, 1]));
    let target = add_line(&mut d, json!([10, -5]), json!([10, 5]));
    let s = sketch(&d);
    let (a_end, b_end) = (end_of(&s, &a), end_of(&s, &b));
    // a's end on the target line, inactive; b's end on it, active.
    let on = |p: &str| json!({"type": "coincident", "point": p, "entity": target});
    let idle = first(&add_constraint(&mut d, on(&a_end)), "constraints", 'k');
    set_active(&mut d, &[idle.as_str()], false);
    let held = first(&add_constraint(&mut d, on(&b_end)), "constraints", 'k');
    // The two ends made one point: both say the same then, and the active
    // one stays, so that the point is still held on the target line.
    add_constraint(
        &mut d,
        json!({"type": "coincident", "point": a_end, "entity": b_end}),
    );
    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    let on_target: Vec<&Value> = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|k| k["type"] == "coincident" && k["entity"] == target.as_str())
        .collect();
    assert_eq!(on_target.len(), 1, "{s}");
    assert_eq!(on_target[0]["id"], held.as_str(), "{s}");
    assert!(on_target[0].get("active").is_none(), "{s}");
    assert!(entry(&s, "constraints", &idle).is_none(), "{s}");
}

#[test]
fn an_equal_spacing_and_its_dimension_are_set_aside_and_activated_together() {
    let mut d = new_sketch();
    let p: Vec<String> = [[0.0, 0.0], [9.0, 0.0], [21.0, 4.0]]
        .iter()
        .map(|at| {
            let made = command(
                &mut d,
                json!({"cmd": "sketch.add_point", "sketch": "F1", "at": at}),
            );
            first(&made, "entities", 'p')
        })
        .collect();
    command(
        &mut d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [p[0]]}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": p,
               "mode": "horizontal", "spacing": 12}),
    );
    let spacing = first(&made, "constraints", 'k');
    let dimension = first(&made, "dimensions", 'k');
    let x = |s: &Value, i: usize| at(s, &p[i])[0];

    // The equal spacing set aside takes its dimension with it.
    let result = set_active(&mut d, &[spacing.as_str()], false);
    assert_eq!(result["made"], json!([spacing, dimension]), "{result}");
    let s = sketch(&d);
    assert_eq!(
        entry(&s, "dimensions", &dimension).unwrap()["active"],
        false
    );
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": p[2], "to": [30, 4]}),
    );
    assert!((x(&sketch(&d), 2) - 30.0).abs() < 1e-6, "nothing holds it");

    // Activating the dimension alone activates the equal spacing it applies
    // with.
    let result = set_active(&mut d, &[dimension.as_str()], true);
    assert_eq!(result["made"], json!([spacing, dimension]), "{result}");
    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    assert!(
        (x(&s, 1) - 12.0).abs() < 1e-6 && (x(&s, 2) - 24.0).abs() < 1e-6,
        "{s}"
    );
    assert!(
        entry(&s, "constraints", &spacing)
            .unwrap()
            .get("active")
            .is_none()
    );

    // The dimension set aside alone leaves the equal spacing.
    let result = set_active(&mut d, &[dimension.as_str()], false);
    assert_eq!(result["made"], json!([dimension]), "{result}");
    let s = sketch(&d);
    assert!(
        entry(&s, "constraints", &spacing)
            .unwrap()
            .get("active")
            .is_none()
    );
    assert_eq!(
        entry(&s, "dimensions", &dimension).unwrap()["active"],
        false
    );
    set_active(&mut d, &[dimension.as_str()], true);

    // A file whose equal spacing alone is inactive: its dimension is shown
    // inactive, with the equal spacing's reason, and activating the
    // dimension applies both.
    let mut file: Value = serde_json::from_str(&d.to_json()).unwrap();
    for k in file["features"][0]["constraints"].as_array_mut().unwrap() {
        if k["id"] == spacing.as_str() {
            k["active"] = json!(false);
            k["inactive_reason"] = json!("not met in the imported file");
        }
    }
    let mut loaded = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    loaded.recompute();
    let s = sketch(&loaded);
    let k = entry(&s, "dimensions", &dimension).unwrap();
    assert_eq!(
        (k["active"].clone(), k["inactive_reason"].clone()),
        (json!(false), json!("not met in the imported file")),
        "{k}"
    );
    let result = set_active(&mut loaded, &[dimension.as_str()], true);
    assert_eq!(result["made"], json!([spacing]), "{result}");
    let s = sketch(&loaded);
    assert!(
        entry(&s, "dimensions", &dimension)
            .unwrap()
            .get("active")
            .is_none(),
        "{s}"
    );
}

#[test]
fn extend_keeps_the_values_of_inactive_dimensions() {
    let mut d = new_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
    );
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p1", "end": [40, 0]}),
    );
    let line = first(&made, "entities", 'c');
    let end = first(&made, "entities", 'p');
    add_line(&mut d, json!([60, -10]), json!([60, 10]));
    // An inactive length and an inactive horizontal distance of the 40 mm
    // line, both 50 mm as a file may have them, and an active distance.
    let length = add_dimension(&mut d, 40.0, json!({"type": "length", "line": line}));
    set_active(&mut d, &[length.as_str()], false);
    set_dimension(&mut d, &length, 50.0);
    let across = add_dimension(
        &mut d,
        40.0,
        json!({"type": "horizontal_distance", "a": "p1", "b": end}),
    );
    set_active(&mut d, &[across.as_str()], false);
    set_dimension(&mut d, &across, 50.0);
    let distance = add_dimension(
        &mut d,
        40.0,
        json!({"type": "distance", "a": "p1", "b": end}),
    );
    // Extended to the line at x = 60: the active distance measures the new
    // end, the inactive ones keep their values.
    command(
        &mut d,
        json!({"cmd": "sketch.extend", "sketch": "F1", "curve": line, "at": [39, 0]}),
    );
    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    assert!((measured(&s, &distance) - 60.0).abs() < 1e-9, "{s}");
    assert!((measured(&s, &across) - 50.0).abs() < 1e-9, "{s}");
    assert!((measured(&s, &length) - 50.0).abs() < 1e-9, "{s}");
    for id in [&length, &across] {
        assert_eq!(entry(&s, "dimensions", id).unwrap()["active"], false);
    }
}

#[test]
fn an_inactive_driven_dimension_keeps_its_reference_measured() {
    let mut d = new_sketch();
    // A 60 x 40 rectangle (its height d2) and a driven length of its left
    // side, whose reference a parameter uses.
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0],
               "width": 60, "height": 40}),
    );
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": "c4"}, "driven": true}),
    );
    let driven = first(&added, "dimensions", 'k');
    let reference = added["parameters"][0].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "twice", "value": format!("{reference} * 2")}),
    );
    set_active(&mut d, &[driven.as_str()], false);
    // The height changes: the inactive driven length still measures it, so
    // its reference and what uses it are never stale.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d2", "value": "25 mm"}),
    );
    let s = sketch(&d);
    assert_eq!(entry(&s, "dimensions", &driven).unwrap()["active"], false);
    assert!((measured(&s, &driven) - 25.0).abs() < 1e-9, "{s}");
    assert!((parameter_value(&d, &reference) - 25.0).abs() < 1e-9);
    assert!((parameter_value(&d, "twice") - 50.0).abs() < 1e-9);
}
