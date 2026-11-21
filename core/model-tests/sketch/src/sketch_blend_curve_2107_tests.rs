// SPDX-License-Identifier: MIT
//! Blend Curve (mitcad#2107) through the JSON api: a spline that joins two
//! curve ends with a tangent or a smooth (curvature) constraint at each,
//! and stays joined when a curve is dragged or dimensioned again.

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

fn line(d: &mut Document<MockKernel>, start: [f64; 2], end: [f64; 2]) -> String {
    let made = command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
    );
    made["made"][0].as_str().unwrap().to_owned()
}

/// A line's solved ends.
fn ends(s: &Value, id: &str) -> ([f64; 2], [f64; 2]) {
    let g = &entity(s, id)["geometry"];
    (xy(&g["start"]), xy(&g["end"]))
}

/// A spline's solved control points.
fn control(s: &Value, id: &str) -> Vec<[f64; 2]> {
    entity(s, id)["geometry"]["control"]
        .as_array()
        .unwrap()
        .iter()
        .map(xy)
        .collect()
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn sine(u: [f64; 2], v: [f64; 2]) -> f64 {
    (u[0] * v[1] - u[1] * v[0]) / (u[0].hypot(u[1]) * v[0].hypot(v[1]))
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

/// The blend joins the first line's end and the second line's start,
/// leaving and arriving along them (within 1e-6).
fn check_joined(s: &Value, blend: &str, first: &str, second: &str) {
    let c = control(s, blend);
    let n = c.len() - 1;
    let (a0, a1) = ends(s, first);
    let (b0, b1) = ends(s, second);
    assert!(near(c[0], a1), "{c:?} {a1:?}");
    assert!(near(c[n], b0), "{c:?} {b0:?}");
    let leaving = sub(c[1], c[0]);
    let along = sub(a1, a0);
    assert!(sine(leaving, along).abs() < 1e-6, "{leaving:?} {along:?}");
    assert!(leaving[0] * along[0] + leaving[1] * along[1] > 0.0);
    let arriving = sub(c[n], c[n - 1]);
    let into = sub(b1, b0);
    assert!(sine(arriving, into).abs() < 1e-6, "{arriving:?} {into:?}");
    assert!(arriving[0] * into[0] + arriving[1] * into[1] > 0.0);
}

#[test]
fn a_tangent_blend_follows_a_drag_and_a_dimension() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let first = line(&mut d, [0.0, 0.0], [10.0, 0.0]);
    let second = line(&mut d, [20.0, 10.0], [20.0, 20.0]);
    let made = command(
        &mut d,
        json!({"cmd": "sketch.blend_curve", "sketch": "F1", "curve1": first, "at1": [9, 0],
               "curve2": second, "at2": [20, 11]}),
    );
    let blend = made["made"][0].as_str().unwrap().to_owned();
    let constraints = made["constraints"].as_array().unwrap();
    assert_eq!(constraints.len(), 2, "{made}");
    let s = sketch(&d);
    assert_eq!(entity(&s, &blend)["type"], "spline", "{s}");
    assert_eq!(control(&s, &blend).len(), 4, "degree 3");
    let types: Vec<&Value> = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| constraints.contains(&c["id"]))
        .map(|c| &c["type"])
        .collect();
    assert_eq!(types, vec!["tangent", "tangent"], "{s}");
    check_joined(&s, &blend, &first, &second);
    // The first line dragged up: the blend follows, still tangent.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": first, "by": [0, 3]}),
    );
    let s = sketch(&d);
    check_joined(&s, &blend, &first, &second);
    // The second line's length dimensioned again.
    let dim = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "length", "line": second}, "value": 10}),
    );
    let k = dim["dimensions"][0].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": k, "value": 4}),
    );
    let s = sketch(&d);
    let (b0, b1) = ends(&s, &second);
    assert!(((b1[0] - b0[0]).hypot(b1[1] - b0[1]) - 4.0).abs() < 1e-6);
    check_joined(&s, &blend, &first, &second);
}

#[test]
fn a_curvature_blend_is_smooth_at_both_ends_and_one_undo_step() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let first = line(&mut d, [0.0, 0.0], [10.0, 0.0]);
    let second = line(&mut d, [20.0, 10.0], [20.0, 20.0]);
    let made = command(
        &mut d,
        json!({"cmd": "sketch.blend_curve", "sketch": "F1", "curve1": first, "at1": [9, 0],
               "curve2": second, "at2": [20, 11], "continuity": "curvature"}),
    );
    let blend = made["made"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert_eq!(control(&s, &blend).len(), 6, "degree 5");
    let smooth = s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "smooth")
        .count();
    assert_eq!(smooth, 2, "{s}");
    check_joined(&s, &blend, &first, &second);
    // Between lines the curvature at each end is zero: the second control
    // point lies on the line too.
    let c = control(&s, &blend);
    assert!(c[2][1].abs() < 1e-6, "{c:?}");
    assert!((c[3][0] - 20.0).abs() < 1e-6, "{c:?}");
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert!(
        s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["id"] != blend.as_str()),
        "{s}"
    );
}

#[test]
fn a_blend_of_a_circle_is_refused_in_words() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let first = line(&mut d, [0.0, 0.0], [10.0, 0.0]);
    let made = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [30, 0], "radius": 5}),
    );
    let circle = made["made"][0].as_str().unwrap().to_owned();
    let error = d
        .command(
            &json!({"cmd": "sketch.blend_curve", "sketch": "F1", "curve1": first, "at1": [9, 0],
                    "curve2": circle, "at2": [35, 0]})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("a circle has no end to blend from"),
        "{error}"
    );
}
