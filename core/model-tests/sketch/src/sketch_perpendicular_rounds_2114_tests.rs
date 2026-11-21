// SPDX-License-Identifier: MIT
//! Perpendicular with circles and arcs (mitcad#2114, GitHub
//! mitcad/Mitcad#484), with the mock kernel: a line and an arc made
//! perpendicular hold the arc's center on the infinite line, one
//! `perpendicular` row that saves and loads as it is; two arcs made
//! perpendicular cross at right angles, `|c1 c2|^2 = r1^2 + r2^2`, also
//! after a radius changes, and a Concentric of the same two conflicts.
//! Ellipses and splines are refused in words.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::notice::FeatureError;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

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

fn constraint(a: &str, b: &str, kind: &str) -> Value {
    json!({"cmd": "sketch.add_constraint", "sketch": "F1",
           "constraint": {"type": kind, "a": a, "b": b}})
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn made(result: &Value) -> String {
    result["made"][0]
        .as_str()
        .unwrap_or_else(|| panic!("nothing made in {result}"))
        .to_owned()
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

fn at(s: &Value, id: &Value) -> [f64; 2] {
    let p = &entity(s, id.as_str().unwrap())["at"];
    [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// A line's end points.
fn ends(s: &Value, line: &str) -> ([f64; 2], [f64; 2]) {
    let l = entity(s, line);
    (at(s, &l["start"]), at(s, &l["end"]))
}

/// An arc's center and radius.
fn round(s: &Value, arc: &str) -> ([f64; 2], f64) {
    let a = entity(s, arc);
    let c = at(s, &a["center"]);
    (c, dist(c, at(s, &a["start"])))
}

/// The distance of `p` from the infinite line through `a` and `b`.
fn off_line(p: [f64; 2], (a, b): ([f64; 2], [f64; 2])) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / dx.hypot(dy)
}

/// Whether a text names a sketch entity, constraint or dimension by its id
/// (`p15`, `c14`, `k5`).
fn names_an_id(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| {
            let mut chars = word.chars();
            matches!(chars.next(), Some('p' | 'c' | 'k' | 't'))
                && !word[1..].is_empty()
                && word[1..].chars().all(|c| c.is_ascii_digit())
        })
}

fn line(d: &mut Document<MockKernel>, start: [f64; 2], end: [f64; 2]) -> String {
    made(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
    ))
}

/// An arc counter-clockwise round `center` from `start` toward `end`.
fn arc(d: &mut Document<MockKernel>, center: [f64; 2], start: [f64; 2], end: [f64; 2]) -> String {
    made(&command(
        d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center",
               "center": center, "start": start, "end": end}),
    ))
}

#[test]
fn a_line_perpendicular_to_an_arc_passes_through_its_center() {
    let mut d = with_sketch();
    let l = line(&mut d, [1.0, 1.0], [11.0, 2.0]);
    // Far from the line: only the line's extension reaches the center.
    let a = arc(&mut d, [20.0, 6.0], [25.0, 6.0], [20.0, 11.0]);
    let free = sketch(&d)["dof"].as_u64().unwrap();
    let k = command(&mut d, constraint(&l, &a, "perpendicular"));
    let k = k["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    assert_eq!(s["dof"].as_u64().unwrap(), free - 1, "{s}");
    assert!(off_line(round(&s, &a).0, ends(&s, &l)) <= 1e-6, "{s}");
    // One row, and nothing else written in its place.
    let rows = s["constraints"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{s}");
    assert_eq!(rows[0]["type"], "perpendicular", "{s}");

    // Dragging the center takes the line with it.
    let center = entity(&s, &a)["center"].clone();
    let before = ends(&s, &l);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": center, "to": [6, 18]}),
    );
    let s = sketch(&d);
    let moved = ends(&s, &l);
    assert!(
        dist(moved.0, before.0) + dist(moved.1, before.1) > 1.0,
        "{s}"
    );
    assert!(off_line(round(&s, &a).0, moved) <= 1e-6, "{s}");

    // Saved and loaded as the same perpendicular.
    let saved = d.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    let s = sketch(&loaded);
    let rows = s["constraints"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{s}");
    assert_eq!(
        [
            &rows[0]["id"],
            &rows[0]["type"],
            &rows[0]["a"],
            &rows[0]["b"]
        ],
        [&json!(k), &json!("perpendicular"), &json!(l), &json!(a)],
        "{s}"
    );
    assert!(off_line(round(&s, &a).0, ends(&s, &l)) <= 1e-6, "{s}");
}

#[test]
fn two_perpendicular_arcs_cross_at_right_angles() {
    let mut d = with_sketch();
    let a = arc(&mut d, [1.0, 1.0], [11.0, 1.0], [1.0, 11.0]);
    let b = arc(&mut d, [16.0, 4.0], [16.0, 10.0], [16.0, -2.0]);
    command(&mut d, constraint(&a, &b, "perpendicular"));
    let radius = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "radius", "curve": a}, "value": 10}),
    );
    let radius = radius["dimensions"][0]
        .as_str()
        .unwrap_or_else(|| panic!("{radius}"))
        .to_owned();
    let check = |s: &Value| {
        assert!(s["error"].is_null(), "{s}");
        let ((c1, r1), (c2, r2)) = (round(s, &a), round(s, &b));
        let d = dist(c1, c2);
        let gap = d * d - r1 * r1 - r2 * r2;
        assert!(gap.abs() <= 1e-6, "|c1 c2|^2 - r1^2 - r2^2 = {gap}: {s}");
        // The radii to a crossing, and so the tangents there, are square.
        let along = [(c2[0] - c1[0]) / d, (c2[1] - c1[1]) / d];
        let x = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
        let h = (r1 * r1 - x * x).sqrt();
        let p = [
            c1[0] + along[0] * x - along[1] * h,
            c1[1] + along[1] * x + along[0] * h,
        ];
        let (u, v) = ([p[0] - c1[0], p[1] - c1[1]], [p[0] - c2[0], p[1] - c2[1]]);
        let angle = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
        assert!(
            (angle.abs() - std::f64::consts::FRAC_PI_2).abs() <= 1e-6,
            "the tangents meet at {angle} rad: {s}"
        );
    };
    check(&sketch(&d));
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": radius,
               "value": 14}),
    );
    let s = sketch(&d);
    assert!((round(&s, &a).1 - 14.0).abs() <= 1e-6, "{s}");
    check(&s);

    // A common center leaves only zero radii: a conflict, refused in words.
    let error = rejected(&mut d, constraint(&a, &b, "concentric"));
    let message = FeatureError::from_text(&error).message;
    assert!(message.contains("Perpendicular"), "{error}");
    assert!(!names_an_id(&message), "{error}");
    check(&sketch(&d));
}

#[test]
fn perpendicular_refuses_splines_and_ellipses_in_words() {
    let mut d = with_sketch();
    let l = line(&mut d, [1.0, 1.0], [11.0, 1.0]);
    // Without a degree, a spline through its fit points.
    let fitted = made(&command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1", "points": [[0, 5], [10, 8], [20, 5]]}),
    ));
    let spline = made(&command(
        &mut d,
        json!({"cmd": "sketch.spline", "sketch": "F1", "degree": 3,
               "points": [[0, 20], [5, 24], [15, 24], [20, 20]]}),
    ));
    let ellipse = made(&command(
        &mut d,
        json!({"cmd": "sketch.ellipse", "sketch": "F1", "center": [30, 10], "major": [36, 10],
               "minor_radius": 3}),
    ));
    for (other, kind) in [
        (&fitted, "a fitted spline"),
        (&spline, "a spline"),
        (&ellipse, "an ellipse"),
    ] {
        let error = rejected(&mut d, constraint(&l, other, "perpendicular"));
        let message = FeatureError::from_text(&error).message;
        assert!(
            message.ends_with(&format!(
                "Perpendicular takes lines, circles and arcs, not {kind}"
            )),
            "{error}"
        );
        assert!(!names_an_id(&message), "{error}");
    }
    // Two lines as before.
    let other = line(&mut d, [1.0, 3.0], [4.0, 13.0]);
    command(&mut d, constraint(&l, &other, "perpendicular"));
    let s = sketch(&d);
    let ((a1, a2), (b1, b2)) = (ends(&s, &l), ends(&s, &other));
    let (u, v) = (
        [a2[0] - a1[0], a2[1] - a1[1]],
        [b2[0] - b1[0], b2[1] - b1[1]],
    );
    assert!(
        (u[0] * v[0] + u[1] * v[1]).abs() <= 1e-6 * dist(a1, a2) * dist(b1, b2),
        "{s}"
    );
}
