// SPDX-License-Identifier: MIT
//! Conic curves through the commands (mitcad#2236): `sketch.conic` draws a
//! conic from its ends and shoulder with a Rho, one undo step; the `sketch`
//! query shows it with its spline form; a closed profile of a line and a
//! conic bounds the area of its segment; Rho follows a parameter and
//! `sketch.set_rho` changes it; the refusals are in words; Trim keeps a
//! conic on the same curve.

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

fn entity<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
}

fn xy(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

fn at(s: &Value, point: &Value) -> [f64; 2] {
    xy(&entity(s, point.as_str().unwrap())["at"])
}

/// A sketch with the conic from (0, 0) to (20, 0), shoulder (10, 10).
fn conic(rho: Value) -> (Document<MockKernel>, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let made = command(
        &mut d,
        json!({"cmd": "sketch.conic", "sketch": "F1", "start": [0, 0], "end": [20, 0],
               "apex": [10, 10], "rho": rho}),
    );
    let id = made["made"][0].as_str().unwrap().to_owned();
    (d, id)
}

/// The point of a rational quadratic Bezier at `t`.
fn bezier(c: [[f64; 2]; 3], w: f64, t: f64) -> [f64; 2] {
    let b = [(1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t) * w, t * t];
    let d = b[0] + b[1] + b[2];
    [
        (b[0] * c[0][0] + b[1] * c[1][0] + b[2] * c[2][0]) / d,
        (b[0] * c[0][1] + b[1] * c[1][1] + b[2] * c[2][1]) / d,
    ]
}

#[test]
fn a_conic_is_drawn_and_shown_with_its_spline_form() {
    let (d, c) = conic(json!(0.7));
    assert_eq!(d.undo_label(), Some("Add Conic to Sketch1"));
    let s = sketch(&d);
    let e = entity(&s, &c);
    assert_eq!(e["type"], "conic", "{e}");
    assert!((e["rho"].as_f64().unwrap() - 0.7).abs() < 1e-12, "{e}");
    assert_eq!(e["degree"], 2);
    let control: Vec<Value> = e["control"].as_array().unwrap().clone();
    assert_eq!(
        control,
        [e["start"].clone(), e["apex"].clone(), e["end"].clone()]
    );
    let w = 0.7 / 0.3;
    assert!((e["weights"][1].as_f64().unwrap() - w).abs() < 1e-9, "{e}");
    assert_eq!(at(&s, &e["apex"]), [10.0, 10.0]);
    // The solved geometry is the rational quadratic of those points.
    assert_eq!(e["geometry"]["degree"], 2, "{e}");
}

/// A line and a parabolic conic (Rho 0.5) bound a region of the area of
/// the parabolic segment: two thirds of the triangle of the chord and the
/// end tangents (20 x 10 / 2 x 2/3 = 66.67 mm²); extruded, it makes a body.
#[test]
fn a_line_and_a_conic_bound_the_segment_s_area() {
    let (mut d, c) = conic(json!(0.5));
    let s = sketch(&d);
    let e = entity(&s, &c).clone();
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": e["start"], "end": e["end"]}),
    );
    let s = sketch(&d);
    let regions = s["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 1, "{s}");
    let area = regions[0]["area"].as_f64().unwrap();
    let exact = 2.0 / 3.0 * (20.0 * 10.0 / 2.0);
    assert!(
        ((area - exact) / exact).abs() < 1e-6,
        "{area} against {exact}"
    );
    let key = regions[0]["key"].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": key}],
               "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}),
    );
    assert_eq!(d.bodies().len(), 1);
}

/// Rho as a parameter: the conic follows it, `sketch.set_rho` changes it
/// (a number, an expression, back to a number); a value outside (0, 1) is
/// refused in words.
#[test]
fn rho_follows_a_parameter_and_is_changed() {
    let mut d = Document::new(MockKernel::default());
    command(
        &mut d,
        json!({"cmd": "add_parameter", "name": "shoulder", "expression": "0.3", "unit": ""}),
    );
    command(&mut d, json!({"cmd": "sketch.create"}));
    let made = command(
        &mut d,
        json!({"cmd": "sketch.conic", "sketch": "F1", "start": [0, 0], "end": [20, 0],
               "apex": [10, 10], "rho": "shoulder"}),
    );
    let c = made["made"][0].as_str().unwrap().to_owned();
    let mid = |d: &Document<MockKernel>| {
        let s = sketch(d);
        let e = entity(&s, &c).clone();
        let rho = e["rho"].as_f64().unwrap();
        let points = [at(&s, &e["start"]), at(&s, &e["apex"]), at(&s, &e["end"])];
        (rho, bezier(points, rho / (1.0 - rho), 0.5))
    };
    let s = sketch(&d);
    assert_eq!(entity(&s, &c)["rho_parameter"], "shoulder");
    let (rho, p) = mid(&d);
    assert!((rho - 0.3).abs() < 1e-12);
    // The middle of a conic lies Rho of the way from the chord's middle to
    // the shoulder.
    assert!((p[1] - 0.3 * 10.0).abs() < 1e-9, "{p:?}");
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "shoulder", "expression": "0.6"}),
    );
    let (rho, p) = mid(&d);
    assert!((rho - 0.6).abs() < 1e-12);
    assert!((p[1] - 6.0).abs() < 1e-9, "{p:?}");
    // An expression of the parameter.
    command(
        &mut d,
        json!({"cmd": "sketch.set_rho", "sketch": "F1", "curve": c, "rho": "shoulder / 2"}),
    );
    assert_eq!(d.undo_label(), Some("Change Rho in Sketch1"));
    let (rho, _) = mid(&d);
    assert!((rho - 0.3).abs() < 1e-12);
    // Outside (0, 1): refused, nothing changed.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.set_rho", "sketch": "F1", "curve": c, "rho": 1.2}),
    );
    assert!(error.contains("between 0 and 1"), "{error}");
    let (rho, _) = mid(&d);
    assert!((rho - 0.3).abs() < 1e-12);
    // Dragging the shoulder reshapes the conic: its solved curve has the
    // shoulder where it was dragged to.
    let s = sketch(&d);
    let apex = entity(&s, &c)["apex"].as_str().unwrap().to_owned();
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": apex, "to": [10, 20]}),
    );
    let s = sketch(&d);
    let control = &entity(&s, &c)["geometry"]["control"];
    let moved = xy(&control[1]);
    assert!(
        (moved[0] - 10.0).abs() < 1e-6 && (moved[1] - 20.0).abs() < 1e-6,
        "{control}"
    );
}

#[test]
fn conics_without_a_shape_are_refused_in_words() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let draw = |d: &mut Document<MockKernel>, apex: Value, rho: Value| {
        rejected(
            d,
            json!({"cmd": "sketch.conic", "sketch": "F1", "start": [0, 0], "end": [20, 0],
                   "apex": apex, "rho": rho}),
        )
    };
    let e = draw(&mut d, json!([10, 10]), json!(0.0));
    assert!(e.contains("Rho must be between 0 and 1"), "{e}");
    let e = draw(&mut d, json!([10, 10]), json!(1.0));
    assert!(e.contains("Rho must be between 0 and 1"), "{e}");
    let e = draw(&mut d, json!([10, 0]), json!(0.5));
    assert!(e.contains("on the line through its ends"), "{e}");
    let e = draw(&mut d, json!([0, 0]), json!(0.5));
    assert!(e.contains("shoulder is at one of its ends"), "{e}");
    let e = rejected(
        &mut d,
        json!({"cmd": "sketch.conic", "sketch": "F1", "start": [0, 0], "end": [0, 0],
               "apex": [10, 10]}),
    );
    assert!(e.contains("start and end are at the same place"), "{e}");
    assert!(sketch(&d)["entities"].as_array().unwrap().is_empty());
}

/// Trim keeps a conic: the piece left is a conic on the same curve, with
/// a new shoulder and a Rho of its own.
#[test]
fn a_trimmed_conic_stays_a_conic_on_the_same_curve() {
    let (mut d, c) = conic(json!(0.5));
    // A vertical line through x = 15 crosses the parabola y = x (20 - x) / 20.
    command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [15, -5], "end": [15, 20]}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": c, "at": [18, 1.8]}),
    );
    let s = sketch(&d);
    let e = entity(&s, &c);
    assert_eq!(e["type"], "conic", "{e}");
    let rho = e["rho"].as_f64().unwrap();
    let points = [at(&s, &e["start"]), at(&s, &e["apex"]), at(&s, &e["end"])];
    assert!((points[2][0] - 15.0).abs() < 1e-6, "{points:?}");
    for i in 0..=20 {
        let p = bezier(points, rho / (1.0 - rho), f64::from(i) / 20.0);
        let y = p[0] * (20.0 - p[0]) / 20.0;
        assert!((p[1] - y).abs() < 1e-6, "{p:?}");
    }
}

fn fillet_corner(rho: f64, right: bool) -> (Document<MockKernel>, String, String, String) {
    let (mut doc, conic) = conic(json!(rho));
    let before = sketch(&doc);
    let end = if right { "end" } else { "start" };
    let corner = entity(&before, &conic)[end].as_str().unwrap().to_owned();
    let result = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": corner,
               "end": if right { [8, 0] } else { [12, 0] }}),
    );
    let line = result["made"][0].as_str().unwrap().to_owned();
    (doc, conic, line, corner)
}

#[test]
fn fillet_trims_a_parabolic_or_elliptical_conic_to_an_exact_piece() {
    const CONTROL: [[f64; 2]; 3] = [[0.0, 0.0], [10.0, 10.0], [20.0, 0.0]];
    let distance = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    for (rho, right) in [(0.5, false), (0.3, true)] {
        let (mut doc, conic, line, corner) = fillet_corner(rho, right);
        let before = sketch(&doc);
        let result = command(
            &mut doc,
            if right {
                json!({"cmd": "sketch.fillet", "sketch": "F1", "points": [corner], "radius": 1})
            } else {
                json!({"cmd": "sketch.fillet", "sketch": "F1", "a": line, "b": conic, "radius": 1})
            },
        );
        let after = sketch(&doc);
        assert_eq!(after["conflicts"], json!([]), "{after}");
        let trimmed = entity(&after, &conic);
        assert_eq!(trimmed["type"], "conic");
        let trimmed_rho = trimmed["rho"].as_f64().unwrap();
        let points = ["start", "apex", "end"].map(|key| at(&after, &trimmed[key]));
        for i in 0..=32 {
            let p = bezier(
                points,
                trimmed_rho / (1.0 - trimmed_rho),
                f64::from(i) / 32.0,
            );
            // The original specimen is monotonic in x. Compare each
            // point of the trimmed curve to the original at the same x.
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..53 {
                let middle = (lo + hi) * 0.5;
                if bezier(CONTROL, rho / (1.0 - rho), middle)[0] < p[0] {
                    lo = middle;
                } else {
                    hi = middle;
                }
            }
            let expected = bezier(CONTROL, rho / (1.0 - rho), (lo + hi) * 0.5);
            assert!(
                distance(p, expected) <= 1e-6,
                "Rho {rho}: {p:?} vs {expected:?}"
            );
        }
        let conic_touch = points[if right { 2 } else { 0 }];
        assert!(distance(conic_touch, at(&before, &json!(corner))) > 0.1);
        let line_touch = at(&after, &entity(&after, &line)["start"]);
        let arc = entity(&after, result["made"][0].as_str().unwrap());
        let center = xy(&arc["geometry"]["center"]);
        let arc_ends = ["start", "end"].map(|key| at(&after, &arc[key]));
        for (touch, tangent) in [
            (conic_touch, points[1]),
            (line_touch, [line_touch[0] + 1.0, 0.0]),
        ] {
            assert!(
                arc_ends.iter().any(|&end| distance(touch, end) <= 1e-6),
                "{arc}"
            );
            assert!((distance(touch, center) - 1.0).abs() <= 1e-6, "{arc}");
            let tangent_error = ((touch[0] - center[0]) * (tangent[0] - touch[0])
                + (touch[1] - center[1]) * (tangent[1] - touch[1]))
                .abs()
                / distance(tangent, touch);
            assert!(tangent_error <= 1e-6, "Rho {rho}: {tangent_error}");
        }
        let count = |kind: &str| {
            after["constraints"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["type"] == kind)
                .count()
        };
        assert_eq!(count("tangent"), 2);
        assert_eq!(count("coincident"), 2);
        assert!(doc.undo().is_some());
        assert_eq!(sketch(&doc), before);
    }
}

#[test]
fn a_fillet_radius_that_consumes_a_conic_corner_is_refused() {
    let (mut doc, conic, line, _) = fillet_corner(0.3, false);
    let before = sketch(&doc);
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": conic, "b": line, "radius": 500}),
    );
    assert!(
        error.contains("fillet radius 500 mm does not fit at the conic corner"),
        "{error}"
    );
    assert_eq!(sketch(&doc), before);
}
