// SPDX-License-Identifier: MIT
//! Sketch Fillet at corners of circular arcs (mitcad#1192): a line and an
//! arc, two arcs, picked as the two curves or by the corner point, several
//! corners of a profile of lines and arcs at once, and the refusals.

use serde_json::{Value, json};

use mitcad_model::Document;
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

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn made(result: &Value) -> Vec<String> {
    result["made"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn near(a: &Value, b: [f64; 2]) -> bool {
    close(a[0].as_f64().unwrap(), b[0]) && close(a[1].as_f64().unwrap(), b[1])
}

fn entities(s: &Value) -> &Vec<Value> {
    s["entities"].as_array().unwrap()
}

/// The id of the point at a position.
fn point_id(s: &Value, at: [f64; 2]) -> String {
    entities(s)
        .iter()
        .find(|e| e["type"] == "point" && near(&e["at"], at))
        .unwrap_or_else(|| panic!("no point at {at:?} in {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The centres of the arcs of radius `r`.
fn arc_centers(s: &Value, r: f64) -> Vec<[f64; 2]> {
    entities(s)
        .iter()
        .filter(|e| e["type"] == "arc" && close(e["geometry"]["radius"].as_f64().unwrap(), r))
        .map(|e| {
            let c = &e["geometry"]["center"];
            [c[0].as_f64().unwrap(), c[1].as_f64().unwrap()]
        })
        .collect()
}

/// Whether an arc of radius `r` has its centre at `center`.
fn has_arc(s: &Value, r: f64, center: [f64; 2]) -> bool {
    arc_centers(s, r)
        .iter()
        .any(|c| close(c[0], center[0]) && close(c[1], center[1]))
}

/// How many constraints of a type the sketch has.
fn count(s: &Value, kind: &str) -> usize {
    s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == kind)
        .count()
}

/// How many dimensions of a type the sketch has.
fn dimensions(s: &Value, kind: &str) -> usize {
    s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|k| k["type"] == kind)
        .count()
}

fn parameter(result: &Value) -> String {
    let parameters = result["parameters"].as_array().unwrap();
    assert_eq!(parameters.len(), 1, "{result}");
    parameters[0].as_str().unwrap().to_owned()
}

/// The curves of a quarter ring about the fixed origin point: the outer
/// arc of r 30 and the inner arc of r 10 counter-clockwise from the x
/// axis to the y axis, the line from (10, 0) to (30, 0) and the line
/// from (0, 10) to (0, 30), fully constrained.
struct Ring {
    outer: String,
    inner: String,
    bottom: String,
    left: String,
}

fn quarter_ring(d: &mut Document<MockKernel>) -> Ring {
    command(
        d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
    );
    let origin = point_id(&sketch(d), [0.0, 0.0]);
    let arc = |d: &mut Document<MockKernel>, r: f64| {
        made(&command(
            d,
            json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": origin,
                   "start": [r, 0], "end": [0, r]}),
        ))[0]
            .clone()
    };
    let outer = arc(d, 30.0);
    let inner = arc(d, 10.0);
    let s = sketch(d);
    let [p10, p30, q10, q30] =
        [[10.0, 0.0], [30.0, 0.0], [0.0, 10.0], [0.0, 30.0]].map(|at| point_id(&s, at));
    let bottom = made(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": p10, "end": p30}),
    ))[0]
        .clone();
    let left = made(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": q10, "end": q30}),
    ))[0]
        .clone();
    for (curve, value) in [(&outer, 30), (&inner, 10)] {
        command(
            d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": {"type": "radius", "curve": curve}, "value": value}),
        );
    }
    for constraint in [
        json!({"type": "horizontal", "line": bottom}),
        json!({"type": "vertical", "line": left}),
        json!({"type": "horizontal_points", "a": origin, "b": p30}),
        json!({"type": "vertical_points", "a": origin, "b": q30}),
    ] {
        command(
            d,
            json!({"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": constraint}),
        );
    }
    assert_eq!(sketch(d)["dof"], 0, "{}", sketch(d)["fully_constrained"]);
    Ring {
        outer,
        inner,
        bottom,
        left,
    }
}

/// Where the centre of a fillet of `r` lies at the corners of the quarter
/// ring on the x axis: inside the outer circle and outside the inner one.
fn outer_corner(r: f64) -> [f64; 2] {
    [(900.0 - 60.0 * r).sqrt(), r]
}

fn inner_corner(r: f64) -> [f64; 2] {
    [(100.0 + 20.0 * r).sqrt(), r]
}

#[test]
fn line_and_arc_corners_inside_and_outside_the_arcs_circle() {
    let mut d = with_sketch();
    let ring = quarter_ring(&mut d);
    // The corner of the bottom line and the outer arc, picked as the two
    // curves: the fillet lies inside the arc's circle (the ring's convex
    // corner).
    let fillet = command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": ring.bottom, "b": ring.outer,
               "radius": 2}),
    );
    assert_eq!(made(&fillet).len(), 1, "{fillet}");
    let outer_radius = parameter(&fillet);
    let s = sketch(&d);
    assert!(
        has_arc(&s, 2.0, outer_corner(2.0)),
        "{:?}",
        arc_centers(&s, 2.0)
    );
    // The outer arc keeps its centre and radius, the line its direction;
    // both end where the fillet touches them, and the corner stays as a
    // point on both.
    assert!(has_arc(&s, 30.0, [0.0, 0.0]), "{:?}", arc_centers(&s, 30.0));
    point_id(&s, [outer_corner(2.0)[0], 0.0]);
    point_id(&s, [30.0, 0.0]);
    assert_eq!(count(&s, "tangent"), 2, "{}", s["constraints"]);
    assert_eq!(dimensions(&s, "radius"), 3, "{}", s["dimensions"]);
    assert_eq!(s["dof"], 0, "{}", s["fully_constrained"]);
    // The corner of the bottom line and the inner arc, picked as the two
    // curves among `curves`: the fillet lies outside the arc's circle (the
    // concave side of the arc).
    let fillet = command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "curves": [ring.inner, ring.bottom],
               "radius": 3}),
    );
    assert_eq!(made(&fillet).len(), 1, "{fillet}");
    let s = sketch(&d);
    assert!(
        has_arc(&s, 3.0, inner_corner(3.0)),
        "{:?}",
        arc_centers(&s, 3.0)
    );
    assert!(has_arc(&s, 10.0, [0.0, 0.0]), "{:?}", arc_centers(&s, 10.0));
    point_id(&s, [10.0, 0.0]);
    assert_eq!(s["dof"], 0, "{}", s["fully_constrained"]);
    assert_eq!(
        s["regions"].as_array().unwrap().len(),
        1,
        "{}",
        s["regions"]
    );
    // The radius dimension drives the fillet.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": outer_radius, "value": 4}),
    );
    let s = sketch(&d);
    assert!(
        has_arc(&s, 4.0, outer_corner(4.0)),
        "{:?}",
        arc_centers(&s, 4.0)
    );
    assert!(
        has_arc(&s, 3.0, inner_corner(3.0)),
        "{:?}",
        arc_centers(&s, 3.0)
    );
    assert_eq!(s["dof"], 0);
    // Undo takes back the radius change, then the second fillet in one
    // step.
    command(&mut d, json!({"cmd": "undo"}));
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    assert!(
        arc_centers(&s, 3.0).is_empty(),
        "{:?}",
        arc_centers(&s, 3.0)
    );
    assert_eq!(arc_centers(&s, 2.0).len(), 1);
}

#[test]
fn line_and_arc_corner_by_its_point() {
    let mut d = with_sketch();
    quarter_ring(&mut d);
    let corner = point_id(&sketch(&d), [0.0, 30.0]);
    let fillet = command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "points": [corner], "radius": 2}),
    );
    assert_eq!(made(&fillet).len(), 1, "{fillet}");
    let s = sketch(&d);
    let [x, y] = outer_corner(2.0);
    assert!(has_arc(&s, 2.0, [y, x]), "{:?}", arc_centers(&s, 2.0));
    // The corner point stays, on the line and on the arc's circle.
    assert_eq!(point_id(&s, [0.0, 30.0]), corner);
    assert_eq!(s["dof"], 0, "{}", s["fully_constrained"]);
}

#[test]
fn all_corners_of_a_profile_of_lines_and_arcs_at_once() {
    let mut d = with_sketch();
    let ring = quarter_ring(&mut d);
    let fillet = command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1",
               "curves": [ring.bottom, ring.outer, ring.left, ring.inner], "radius": 2}),
    );
    assert_eq!(made(&fillet).len(), 4, "{fillet}");
    let radius = parameter(&fillet);
    let s = sketch(&d);
    assert_eq!(arc_centers(&s, 2.0).len(), 4, "{:?}", arc_centers(&s, 2.0));
    assert_eq!(count(&s, "tangent"), 8, "{}", s["constraints"]);
    assert_eq!(count(&s, "equal"), 3, "{}", s["constraints"]);
    // The ring's two and the fillets' one.
    assert_eq!(dimensions(&s, "radius"), 3, "{}", s["dimensions"]);
    assert_eq!(s["dof"], 0, "{}", s["fully_constrained"]);
    assert_eq!(
        s["regions"].as_array().unwrap().len(),
        1,
        "{}",
        s["regions"]
    );
    // The one radius moves all four, inside and outside the arcs.
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": radius, "value": 3}),
    );
    let s = sketch(&d);
    let ([ox, oy], [ix, iy]) = (outer_corner(3.0), inner_corner(3.0));
    for center in [[ox, oy], [oy, ox], [ix, iy], [iy, ix]] {
        assert!(
            has_arc(&s, 3.0, center),
            "{center:?}: {:?}",
            arc_centers(&s, 3.0)
        );
    }
    assert_eq!(s["dof"], 0);
    // The same corners by their points.
    let mut d = with_sketch();
    quarter_ring(&mut d);
    let s = sketch(&d);
    let corners = [[10.0, 0.0], [30.0, 0.0], [0.0, 30.0], [0.0, 10.0]].map(|at| point_id(&s, at));
    let fillet = command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "points": corners, "radius": 2}),
    );
    assert_eq!(made(&fillet).len(), 4, "{fillet}");
    let s = sketch(&d);
    let ([ox, oy], [ix, iy]) = (outer_corner(2.0), inner_corner(2.0));
    for center in [[ox, oy], [oy, ox], [ix, iy], [iy, ix]] {
        assert!(
            has_arc(&s, 2.0, center),
            "{center:?}: {:?}",
            arc_centers(&s, 2.0)
        );
    }
    assert_eq!(s["dof"], 0, "{}", s["fully_constrained"]);
}

/// A lens: the arcs of r 10 about the fixed points (0, 0) and (10, 0)
/// between the circles' crossings (5, ±√75). Returns the two arcs and the
/// top and bottom corner points.
fn lens(d: &mut Document<MockKernel>) -> ([String; 2], [String; 2]) {
    let h = 75f64.sqrt();
    for y in [h, -h] {
        command(
            d,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [5.0, y]}),
        );
    }
    let s = sketch(d);
    let (top, bottom) = (point_id(&s, [5.0, h]), point_id(&s, [5.0, -h]));
    let right = made(&command(
        d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": bottom, "through": [10, 0], "end": top}),
    ))[0]
        .clone();
    let left = made(&command(
        d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": top, "through": [0, 0], "end": bottom}),
    ))[0]
        .clone();
    let s = sketch(d);
    let centers = [[0.0, 0.0], [10.0, 0.0]].map(|at| point_id(&s, at));
    command(
        d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": centers}),
    );
    for curve in [&right, &left] {
        command(
            d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": {"type": "radius", "curve": curve}, "value": 10}),
        );
    }
    assert_eq!(sketch(d)["dof"], 0, "{}", sketch(d)["fully_constrained"]);
    ([right, left], [top, bottom])
}

#[test]
fn arc_and_arc_corners_by_point_and_by_curves() {
    let mut d = with_sketch();
    let ([right, left], [top, _]) = lens(&mut d);
    // The two arcs end together at both corners: as "a" and "b" they name
    // no one corner.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": right, "b": left, "radius": 1}),
    );
    assert!(error.contains("pick the corner point"), "{error}");
    // The top corner by its point: inside both circles, 9 from both
    // centres.
    let fillet = command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "points": [top], "radius": 1}),
    );
    assert_eq!(made(&fillet).len(), 1, "{fillet}");
    let h = 56f64.sqrt();
    let s = sketch(&d);
    assert!(has_arc(&s, 1.0, [5.0, h]), "{:?}", arc_centers(&s, 1.0));
    assert!(has_arc(&s, 10.0, [0.0, 0.0]) && has_arc(&s, 10.0, [10.0, 0.0]));
    assert_eq!(s["dof"], 0, "{}", s["fully_constrained"]);
    // Now they end together at the bottom only: the two curves name it.
    let fillet = command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": left, "b": right, "radius": 2}),
    );
    let radius = parameter(&fillet);
    let s = sketch(&d);
    assert!(
        has_arc(&s, 2.0, [5.0, -39f64.sqrt()]),
        "{:?}",
        arc_centers(&s, 2.0)
    );
    assert_eq!(s["dof"], 0, "{}", s["fully_constrained"]);
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": radius, "value": 1}),
    );
    let s = sketch(&d);
    assert!(has_arc(&s, 1.0, [5.0, -h]), "{:?}", arc_centers(&s, 1.0));
    assert_eq!(arc_centers(&s, 1.0).len(), 2);
    // Both corners at once, by the two curves.
    let mut d = with_sketch();
    let ([right, left], _) = lens(&mut d);
    let fillet = command(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "curves": [right, left], "radius": 1}),
    );
    assert_eq!(made(&fillet).len(), 2, "{fillet}");
    let s = sketch(&d);
    assert!(has_arc(&s, 1.0, [5.0, h]) && has_arc(&s, 1.0, [5.0, -h]));
    assert_eq!(count(&s, "equal"), 1);
    assert_eq!(s["dof"], 0, "{}", s["fully_constrained"]);
}

#[test]
fn arc_fillets_that_do_not_fit_or_have_no_corner_are_refused() {
    // A radius past the end of the line: the line and the largest radius
    // named; nothing changes.
    let mut d = with_sketch();
    let ring = quarter_ring(&mut d);
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": ring.bottom, "b": ring.outer,
               "radius": 14}),
    );
    assert!(
        error.contains(&format!("does not fit on {}", ring.bottom)),
        "{error}"
    );
    assert!(error.contains("must be less than 13.333333 mm"), "{error}");
    assert!(arc_centers(&sketch(&d), 14.0).is_empty());
    // All four corners: each line has two of them, which meet at r 10.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1",
               "curves": [ring.outer, ring.inner, ring.bottom, ring.left], "radius": 12}),
    );
    assert!(error.contains("must be less than 10 mm"), "{error}");
    // A half disc: inside the circle of r 5, no fillet of more than 2.5
    // touches both the arc and its diameter.
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
    );
    let origin = point_id(&sketch(&d), [0.0, 0.0]);
    let arc = made(&command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": origin,
               "start": [5, 0], "end": [-5, 0]}),
    ))[0]
        .clone();
    let s = sketch(&d);
    let ends = [[5.0, 0.0], [-5.0, 0.0]].map(|at| point_id(&s, at));
    let line = made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": ends[0], "end": ends[1]}),
    ))[0]
        .clone();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "points": [ends[0]], "radius": 3}),
    );
    assert!(
        error.contains(&format!("does not fit on {arc} at its corner with {line}")),
        "{error}"
    );
    assert!(error.contains("must be less than 2.5 mm"), "{error}");
    // Picked as the two curves, the half disc's line and arc name two
    // corners.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": line, "b": arc, "radius": 1}),
    );
    assert!(error.contains("pick the corner point"), "{error}");
    // A line that runs on into an arc tangent to it makes no corner.
    let mut d = with_sketch();
    let line = made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [-10, 10], "end": [0, 10]}),
    ))[0]
        .clone();
    let joint = point_id(&sketch(&d), [0.0, 10.0]);
    let h = 50f64.sqrt();
    let arc = made(&command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": joint, "through": [h, h], "end": [10, 0]}),
    ))[0]
        .clone();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": line, "b": arc, "radius": 1}),
    );
    assert!(error.contains("no corner to round"), "{error}");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "points": [joint], "radius": 1}),
    );
    assert!(error.contains("are tangent there"), "{error}");
    // A circle has no corners; nor does an arc and a line it does not end
    // with.
    let circle = made(&command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [40, 0],
               "radius": 5}),
    ))[0]
        .clone();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": line, "b": circle, "radius": 1}),
    );
    assert!(
        error.contains("is a circle: Sketch Fillet rounds corners of lines and circular arcs"),
        "{error}"
    );
    let other = made(&command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [20, -10], "end": [20, 10]}),
    ))[0]
        .clone();
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": other, "b": arc, "radius": 1}),
    );
    assert!(error.contains("do not end at one point"), "{error}");
}

/// A corner as a Trim leaves it: the arc of r 20 about the fixed origin
/// from (20, 0) to (0, 20), and a line from the fixed point (0, 40) down
/// to the arc whose end is a point of its own held on the arc
/// (point-on-curve), 0.002 mm from the arc's end, fully constrained.
/// Returns the line, the arc, the line's end and the arc's end.
fn trimmed_corner(d: &mut Document<MockKernel>) -> [String; 4] {
    command(
        d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
    );
    let origin = point_id(&sketch(d), [0.0, 0.0]);
    let arc = made(&command(
        d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": origin,
               "start": [20, 0], "end": [0, 20]}),
    ))[0]
        .clone();
    let line = made(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 40],
               "end": [0.002, 20]}),
    ))[0]
        .clone();
    let s = sketch(d);
    let [start, arc_end, top, line_end] =
        [[20.0, 0.0], [0.0, 20.0], [0.0, 40.0], [0.002, 20.0]].map(|at| point_id(&s, at));
    command(
        d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1",
               "constraint": {"type": "coincident", "point": line_end, "entity": arc}}),
    );
    command(
        d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "radius", "curve": arc}, "value": 20}),
    );
    command(
        d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "horizontal_distance", "a": origin, "b": line_end},
               "value": 0.002}),
    );
    command(
        d,
        json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [top]}),
    );
    for constraint in [
        json!({"type": "horizontal_points", "a": origin, "b": start}),
        json!({"type": "vertical_points", "a": origin, "b": arc_end}),
    ] {
        command(
            d,
            json!({"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": constraint}),
        );
    }
    let s = sketch(d);
    assert_eq!(s["dof"], 0, "{}", s["fully_constrained"]);
    assert_ne!(line_end, arc_end);
    [line, arc, line_end, arc_end]
}

#[test]
fn a_corner_of_two_ends_held_together_as_trim_leaves_it() {
    // By the line's end, by the arc's end, by the two curves in `curves`
    // and as "a" and "b": each the one corner.
    for pick in 0..4 {
        let mut d = with_sketch();
        let [line, arc, line_end, arc_end] = trimmed_corner(&mut d);
        let fillet = command(
            &mut d,
            match pick {
                0 => json!({"cmd": "sketch.fillet", "sketch": "F1", "points": [line_end],
                            "radius": 2}),
                1 => json!({"cmd": "sketch.fillet", "sketch": "F1", "points": [arc_end],
                            "radius": 2}),
                2 => json!({"cmd": "sketch.fillet", "sketch": "F1", "curves": [line, arc],
                            "radius": 2}),
                _ => json!({"cmd": "sketch.fillet", "sketch": "F1", "a": arc, "b": line,
                            "radius": 2}),
            },
        );
        assert_eq!(made(&fillet).len(), 1, "{pick}: {fillet}");
        let s = sketch(&d);
        // Outside the arc's circle (the line rises from it): 22 from the
        // origin, above the arc's end.
        let centers = arc_centers(&s, 2.0);
        assert_eq!(centers.len(), 1, "{pick}: {centers:?}");
        let [x, y] = centers[0];
        assert!(close(x.hypot(y), 22.0), "{pick}: {centers:?}");
        assert!(x > 1.9 && y > 20.0, "{pick}: {centers:?}");
        assert_eq!(count(&s, "tangent"), 2, "{pick}: {}", s["constraints"]);
        // Both old ends stay, each on its own curve, still held together.
        point_id(&s, [0.0, 20.0]);
        assert_eq!(point_id(&s, [0.002, 20.0]), line_end, "{pick}");
        assert_eq!(s["dof"], 0, "{pick}: {}", s["fully_constrained"]);
        command(
            &mut d,
            json!({"cmd": "set_parameter", "name": parameter(&fillet), "value": 3}),
        );
        let s = sketch(&d);
        let centers = arc_centers(&s, 3.0);
        assert!(
            centers.len() == 1 && close(centers[0][0].hypot(centers[0][1]), 23.0),
            "{pick}: {centers:?}"
        );
        assert_eq!(s["dof"], 0, "{pick}");
    }
}
