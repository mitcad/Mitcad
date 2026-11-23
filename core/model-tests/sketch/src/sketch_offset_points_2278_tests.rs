// SPDX-License-Identifier: MIT
//! Points held on derived offsets of splines and ellipses (mitcad#2278): a
//! Coincident of a point of the sketch with a curve or an end of such an
//! offset, and a drawing tool started at its end, hold the point on the
//! exact offset of the source, `S(t) + d n(t)`, as the source moves and the
//! distance changes; a trimmed inner corner and constraints other than
//! Coincident are refused in words.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::sketch::geometry::{Curve2, Nurbs};
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

fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn regions(doc: &Document<MockKernel>) -> usize {
    sketch(doc)["regions"].as_array().unwrap().len()
}

fn entity(s: &Value, id: &str) -> Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("{id} not in {}", s["entities"]))
        .clone()
}

fn text(v: &Value) -> String {
    v.as_str()
        .unwrap_or_else(|| panic!("{v} is no id"))
        .to_owned()
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    let p = entity(s, id)["at"].clone();
    [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()]
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The solved curve of an entity of the `sketch` query.
fn curve(s: &Value, id: &str) -> Curve2 {
    let e = entity(s, id);
    let g = &e["geometry"];
    let p = |v: &Value| [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()];
    let f = |v: &Value| v.as_f64().unwrap();
    match e["type"].as_str().unwrap() {
        "line" => Curve2::Line {
            a: p(&g["start"]),
            b: p(&g["end"]),
        },
        "arc" => Curve2::Arc {
            center: p(&g["center"]),
            radius: f(&g["radius"]),
            start: f(&g["start_angle"]),
            end: f(&g["end_angle"]),
        },
        "ellipse" => Curve2::Ellipse {
            center: p(&g["center"]),
            major: f(&g["major_radius"]),
            minor: f(&g["minor_radius"]),
            rotation: f(&g["rotation"]),
        },
        "elliptical_arc" => Curve2::EllipticalArc {
            center: p(&g["center"]),
            major: f(&g["major_radius"]),
            minor: f(&g["minor_radius"]),
            rotation: f(&g["rotation"]),
            start: f(&g["start_angle"]),
            end: f(&g["end_angle"]),
        },
        _ => Curve2::Nurbs(Nurbs {
            degree: g["degree"].as_u64().unwrap() as usize,
            control: g["control"].as_array().unwrap().iter().map(p).collect(),
            weights: g["weights"].as_array().unwrap().iter().map(f).collect(),
            knots: g["knots"].as_array().unwrap().iter().map(f).collect(),
        }),
    }
}

/// How far a point is from a curve.
fn off(s: &Value, id: &str, p: [f64; 2]) -> f64 {
    curve(s, id).closest(p).1
}

/// The point of the sketch query that a curve of it starts (`last` false)
/// or ends at: a line's, an arc's and a spline's first or last point.
fn end_of(s: &Value, id: &str, last: bool) -> String {
    let e = entity(s, id);
    let list = match e["type"].as_str().unwrap() {
        "spline" => e["control"].clone(),
        "fitted_spline" => e["points"].clone(),
        _ => json!([e["start"], e["end"]]),
    };
    let list = list.as_array().unwrap();
    text(if last { list.last().unwrap() } else { &list[0] })
}

fn coincident(d: &mut Document<MockKernel>, point: &str, entity: &str) -> Value {
    command(
        d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "coincident", "point": point, "entity": entity}}),
    )
}

fn line(d: &mut Document<MockKernel>, start: &str, end: &str) -> String {
    text(
        &command(
            d,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
        )["made"][0],
    )
}

/// An open fitted spline and its offset by 3 to its right; the spline's
/// id, the offset's and its result's.
fn spline_and_offset(d: &mut Document<MockKernel>) -> (String, String, String) {
    command(
        d,
        json!({"cmd": "sketch.spline", "sketch": "F1",
               "points": [[0, 0], [10, 6], [20, 4], [30, 10]]}),
    );
    let s = sketch(d);
    let spline = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "fitted_spline")
        .map(|e| text(&e["id"]))
        .unwrap();
    let made = command(
        d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": [spline], "distance": 3}),
    );
    let offset = text(&made["constraints"][0]);
    let s = sketch(d);
    assert_eq!(s["offsets"][0]["derived"], true);
    let result = text(&s["offsets"][0]["results"][0]);
    (spline, offset, result)
}

#[test]
fn circle_centre_and_lines_hold_to_a_spline_offset() {
    let mut d = with_sketch();
    let (spline, offset, result) = spline_and_offset(&mut d);
    let s = sketch(&d);
    // A circle whose centre is put on the offset curve, near its middle.
    let near = curve(&s, &result).point(0.5);
    let circle = text(
        &command(
            &mut d,
            json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
                   "center": [near[0] + 0.4, near[1] - 0.3], "radius": 1,
                   "construction": true}),
        )["made"][0],
    );
    let centre = text(&entity(&sketch(&d), &circle)["center"]);
    coincident(&mut d, &centre, &result);
    // Lines from the spline's ends to the offset's: the line tool started at
    // the spline's end and ended at the offset's end makes a point of its
    // own there, held to the offset's end.
    let s = sketch(&d);
    let first = line(
        &mut d,
        &end_of(&s, &spline, false),
        &end_of(&s, &result, false),
    );
    let last = line(
        &mut d,
        &end_of(&s, &spline, true),
        &end_of(&s, &result, true),
    );
    let check = |d: &Document<MockKernel>, distance: f64| {
        let s = sketch(d);
        let c = at(&s, &centre);
        assert!(
            (off(&s, &spline, c) - distance).abs() < 1e-6,
            "centre {c:?}"
        );
        assert!(off(&s, &result, c) < 2e-4, "centre {c:?} off the offset");
        for (l, last) in [(&first, false), (&last, true)] {
            let tip = at(&s, &end_of(&s, l, true));
            let offset_end = at(&s, &end_of(&s, &result, last));
            assert!(dist(tip, offset_end) < 1e-7, "{l}: {tip:?} {offset_end:?}");
            // The exact offset of the spline's end.
            let spline_end = at(&s, &end_of(&s, &spline, last));
            assert!((dist(tip, spline_end) - distance).abs() < 1e-7);
        }
        // The spline, its offset and the two lines close a profile.
        assert_eq!(regions(d), 1, "{}", s["regions"]);
    };
    check(&d, 3.0);
    // The line's own end point is not the offset's.
    let s = sketch(&d);
    assert_ne!(end_of(&s, &first, true), end_of(&s, &result, false));
    // The source moves: everything follows on the exact offset.
    let fit = text(&entity(&s, &spline)["points"][2]);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": fit, "to": [20, 1]}),
    );
    check(&d, 3.0);
    let end = text(&entity(&sketch(&d), &spline)["points"][3]);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": end, "to": [32, 14]}),
    );
    check(&d, 3.0);
    // The distance changes.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": offset, "distance": 2}),
    );
    check(&d, 2.0);
    // A drag of the held point moves it along the offset only.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": centre, "to": [12, 0]}),
    );
    check(&d, 2.0);
}

#[test]
fn points_hold_to_an_elliptical_arc_offset() {
    let mut d = with_sketch();
    // The upper half of an ellipse, counter-clockwise from (10, 0) to
    // (-10, 0); its offset to the right of that direction is outside.
    let arc = text(
        &command(
            &mut d,
            json!({"cmd": "sketch.ellipse", "sketch": "F1", "center": [0, 0],
                   "major": [10, 0], "minor_radius": 5, "start": [10, 0], "end": [-10, 0]}),
        )["made"][0],
    );
    let s = sketch(&d);
    assert_eq!(entity(&s, &arc)["type"], "elliptical_arc");
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": [arc], "distance": 2}),
    );
    let offset = text(&made["constraints"][0]);
    let s = sketch(&d);
    assert_eq!(s["offsets"][0]["derived"], true);
    let result = text(&s["offsets"][0]["results"][0]);
    let p = text(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [3, 7.5]}),
        )["entities"][0],
    );
    coincident(&mut d, &p, &result);
    let s = sketch(&d);
    let first = line(
        &mut d,
        &end_of(&s, &arc, false),
        &end_of(&s, &result, false),
    );
    let last = line(&mut d, &end_of(&s, &arc, true), &end_of(&s, &result, true));
    let check = |d: &Document<MockKernel>, distance: f64| {
        let s = sketch(d);
        let q = at(&s, &p);
        assert!((off(&s, &arc, q) - distance).abs() < 1e-6, "{q:?}");
        assert!(off(&s, &result, q) < 2e-4, "{q:?}");
        assert!(q[1] > 0.0, "outside the upper half: {q:?}");
        for (l, last) in [(&first, false), (&last, true)] {
            let tip = at(&s, &end_of(&s, l, true));
            let arc_end = at(&s, &end_of(&s, &arc, last));
            assert!((dist(tip, arc_end) - distance).abs() < 1e-7, "{l}");
            assert!(dist(tip, at(&s, &end_of(&s, &result, last))) < 1e-7);
        }
        assert_eq!(regions(d), 1, "{}", s["regions"]);
    };
    check(&d, 2.0);
    // The ellipse changes its minor radius, then the distance changes.
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "minor_radius", "ellipse": arc}, "value": 4}),
    );
    check(&d, 2.0);
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": offset, "distance": 1.5}),
    );
    check(&d, 1.5);
}

/// A convex outline, counter-clockwise: three lines and a spline arching
/// over the top; the chain of its curves and the bottom line's end point
/// (its corner with the right line).
fn outline(d: &mut Document<MockKernel>) -> (Value, String) {
    let add = |d: &mut Document<MockKernel>, start: Value, end: Value| {
        text(
            &command(
                d,
                json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
            )["made"][0],
        )
    };
    let bottom = add(d, json!([0, 0]), json!([40, 0]));
    let s = sketch(d);
    let (b0, b1) = (end_of(&s, &bottom, false), end_of(&s, &bottom, true));
    let right = add(d, json!(b1), json!([40, 20]));
    let r1 = end_of(&sketch(d), &right, true);
    let top = text(
        &command(
            d,
            json!({"cmd": "sketch.spline", "sketch": "F1", "points": [r1, [20, 28], [0, 20]]}),
        )["made"][0],
    );
    let t1 = end_of(&sketch(d), &top, true);
    let left = add(d, json!(t1), json!(b0));
    (json!([bottom, right, top, left]), b1)
}

#[test]
fn a_point_holds_to_a_round_corner() {
    let mut d = with_sketch();
    let (chain, b1) = outline(&mut d);
    // Outward (to the right of the counter-clockwise chain): round corners
    // about the source's corners.
    let made = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": chain, "distance": 2}),
    );
    let offset = text(&made["constraints"][0]);
    let s = sketch(&d);
    let corners = s["offsets"][0]["corners"].as_array().unwrap().clone();
    assert_eq!(corners.len(), 4);
    // The corner at the bottom right, about the bottom line's end.
    let corner = corners
        .iter()
        .map(|c| text(&c["arc"]))
        .find(|a| match curve(&s, a) {
            Curve2::Arc { center, .. } => dist(center, [40.0, 0.0]) < 1e-9,
            _ => false,
        })
        .unwrap();
    let p = text(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [41.8, -1.0]}),
        )["entities"][0],
    );
    coincident(&mut d, &p, &corner);
    let check = |d: &Document<MockKernel>, distance: f64| {
        let s = sketch(d);
        let q = at(&s, &p);
        let k = at(&s, &b1);
        assert!((dist(q, k) - distance).abs() < 1e-7, "{q:?} {k:?}");
        // On the arc, within its span (outside it the nearest place on the
        // arc would be an end, farther than the tolerance).
        assert!(off(&s, &corner, q) < 1e-7, "{q:?}");
    };
    check(&d, 2.0);
    // The corner moves with its source.
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": b1, "to": [44, -3]}),
    );
    check(&d, 2.0);
    command(
        &mut d,
        json!({"cmd": "sketch.edit_offset", "sketch": "F1", "offset": offset, "distance": 3}),
    );
    check(&d, 3.0);

    // Other constraints on what the offset computes are refused in words.
    let s = sketch(&d);
    let first = text(&s["offsets"][0]["results"][0]);
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "horizontal", "line": first}}),
    );
    assert!(error.contains("takes only a Coincident"), "{error}");
}

#[test]
fn a_point_at_a_trimmed_inner_corner_is_refused() {
    let mut d = with_sketch();
    let (chain, _) = outline(&mut d);
    // Inward (to the left of the counter-clockwise chain): the corners are
    // trimmed where the results cross.
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F1", "curves": chain, "distance": -2}),
    );
    let s = sketch(&d);
    assert_eq!(s["offsets"][0]["derived"], true);
    let rounded = s["offsets"][0]["corners"].as_array().map_or(0, Vec::len);
    assert_eq!(rounded, 0, "inward, no corner is round");
    // The bottom line's result ends where it crosses the right line's.
    let bottom = text(&s["offsets"][0]["results"][0]);
    assert_eq!(entity(&s, &bottom)["type"], "line");
    let trimmed = end_of(&s, &bottom, true);
    let free = text(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [50, 50]}),
        )["entities"][0],
    );
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "coincident", "point": free, "entity": trimmed}}),
    );
    assert!(error.contains("trimmed inner corner"), "{error}");
}
