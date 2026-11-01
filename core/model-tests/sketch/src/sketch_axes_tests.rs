// SPDX-License-Identifier: MIT
//! The sketch's own axes and origin as entities for constraints and
//! dimensions (mitcad#500), with the mock kernel.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::sketch::{Entity, SketchAxis};
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

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
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

fn at(s: &Value, id: &str) -> [f64; 2] {
    let e = entity(s, id);
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

fn measured(s: &Value, id: &str) -> f64 {
    s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))["measured"]
        .as_f64()
        .unwrap()
}

/// A new document with an empty Sketch1 (F1).
fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn axis(d: &mut Document<MockKernel>, which: &str) -> Value {
    command(
        d,
        json!({"cmd": "sketch.axis", "sketch": "F1", "axis": which}),
    )
}

fn axis_id(d: &mut Document<MockKernel>, which: &str) -> String {
    axis(d, which)["entity"].as_str().unwrap().to_owned()
}

fn add_point(d: &mut Document<MockKernel>, xy: [f64; 2]) -> String {
    let added = command(
        d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": xy}),
    );
    added["entities"][0].as_str().unwrap().to_owned()
}

fn constrain(d: &mut Document<MockKernel>, constraint: Value) -> Value {
    command(
        d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": constraint}),
    )
}

fn dimension(d: &mut Document<MockKernel>, dimension: Value, value: &str) -> String {
    let added = command(
        d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": dimension, "value": value}),
    );
    added["dimensions"][0].as_str().unwrap().to_owned()
}

/// The line of a sketch with its point ids.
fn line(d: &mut Document<MockKernel>, a: [f64; 2], b: [f64; 2]) -> (String, String, String) {
    let added = command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": a, "end": b}),
    );
    let made: Vec<String> = added["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    let id = made.iter().find(|m| m.starts_with('c')).unwrap().clone();
    let s = sketch(d);
    let e = entity(&s, &id);
    (
        id,
        e["start"].as_str().unwrap().to_owned(),
        e["end"].as_str().unwrap().to_owned(),
    )
}

#[test]
fn an_axis_or_the_origin_asked_twice_is_one_entity() {
    let mut d = with_sketch();
    let depth = d.undo_depth();
    let x = axis(&mut d, "x");
    assert_eq!(x["new"], true, "{x}");
    let x_line = x["entity"].as_str().unwrap().to_owned();
    assert_eq!(d.undo_depth(), depth + 1);
    assert_eq!(d.undo_label(), Some("Axis in Sketch1"));

    // The line: fixed construction geometry from the origin point along +x,
    // flagged as the axis with its far end.
    let s = sketch(&d);
    let e = entity(&s, &x_line);
    assert_eq!(e["type"], "line", "{e}");
    assert_eq!(e["axis"], "x", "{e}");
    assert_eq!(e["construction"], true, "{e}");
    assert_eq!(e["fixed"], true, "{e}");
    let start = e["start"].as_str().unwrap().to_owned();
    let end = e["end"].as_str().unwrap().to_owned();
    let [ox, oy] = at(&s, &start);
    assert!(close(ox, 0.0) && close(oy, 0.0), "{s}");
    assert_eq!(entity(&s, &start)["fixed"], true);
    assert!(entity(&s, &start).get("axis").is_none(), "{s}");
    assert_eq!(entity(&s, &end)["axis"], "x");
    assert_eq!(entity(&s, &end)["fixed"], true);
    assert!(at(&s, &end)[0] > 0.0 && close(at(&s, &end)[1], 0.0), "{s}");
    assert_eq!(s["dof"], 0, "{s}");
    assert!(s["regions"].as_array().unwrap().is_empty(), "{s}");

    // Asked again: the same line, no new undo step.
    let again = axis(&mut d, "x");
    assert_eq!(
        (&again["entity"], &again["new"]),
        (&json!(x_line), &json!(false))
    );
    assert_eq!(d.undo_depth(), depth + 1);
    // The origin is the axis's start point.
    let origin = axis(&mut d, "origin");
    assert_eq!(
        (&origin["entity"], &origin["new"]),
        (&json!(start), &json!(false))
    );
    // The y axis shares it.
    let y_line = axis_id(&mut d, "y");
    assert_ne!(y_line, x_line);
    let s = sketch(&d);
    assert_eq!(entity(&s, &y_line)["start"], start.as_str());
    assert_eq!(entity(&s, &y_line)["axis"], "y");
    assert_eq!(axis_id(&mut d, "y"), y_line);
    let entities = s["entities"].as_array().unwrap().len();
    assert_eq!(entities, 5, "{s}");

    assert!(
        rejected(
            &mut d,
            json!({"cmd": "sketch.axis", "sketch": "F1", "axis": "z"})
        )
        .contains("unknown axis “z”")
    );
}

#[test]
fn the_origin_alone_is_the_fixed_point_drawing_snaps_to() {
    let mut d = with_sketch();
    // A fixed point at [0, 0] as drawing makes it (mitcad#355) is the origin.
    let made = command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true}),
    );
    let snapped = made["entities"][0].as_str().unwrap().to_owned();
    let origin = axis(&mut d, "origin");
    assert_eq!(
        (&origin["entity"], &origin["new"]),
        (&json!(snapped), &json!(false))
    );
    // An axis made later starts there.
    let x = axis_id(&mut d, "x");
    assert_eq!(entity(&sketch(&d), &x)["start"], snapped.as_str());

    // Without one, the origin is made alone.
    let mut d = with_sketch();
    let origin = axis(&mut d, "origin");
    assert_eq!(origin["new"], true, "{origin}");
    let s = sketch(&d);
    let id = origin["entity"].as_str().unwrap();
    let [ox, oy] = at(&s, id);
    assert!(close(ox, 0.0) && close(oy, 0.0), "{s}");
    assert_eq!(entity(&s, id)["fixed"], true);
    assert_eq!(s["entities"].as_array().unwrap().len(), 1, "{s}");
}

#[test]
fn a_point_dimensioned_from_an_axis_and_a_line_at_an_angle_to_it() {
    let mut d = with_sketch();
    // A point 30 mm from the x axis.
    let p = add_point(&mut d, [12.0, 20.0]);
    let x = axis_id(&mut d, "x");
    let k = dimension(
        &mut d,
        json!({"type": "point_line_distance", "point": p, "line": x}),
        "30",
    );
    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    assert!(close(at(&s, &p)[1], 30.0), "{s}");
    assert!(close(measured(&s, &k), 30.0), "{s}");

    // A line at 25 degrees to the x axis.
    let (l, start, end) = line(&mut d, [10.0, 5.0], [40.0, 20.0]);
    let k = dimension(
        &mut d,
        json!({"type": "angle", "a": x, "b": l, "reverse_a": false, "reverse_b": false}),
        "25 deg",
    );
    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    assert!(close(measured(&s, &k), 25f64.to_radians()), "{s}");
    let [ax, ay] = at(&s, &start);
    let [bx, by] = at(&s, &end);
    assert!(close((by - ay).atan2(bx - ax), 25f64.to_radians()), "{s}");

    // The value changes it as any dimension's.
    command(
        &mut d,
        json!({"cmd": "sketch.set_dimension", "sketch": "F1", "dimension": k, "value": "40 deg"}),
    );
    let s = sketch(&d);
    let [ax, ay] = at(&s, &start);
    let [bx, by] = at(&s, &end);
    assert!(close((by - ay).atan2(bx - ax), 40f64.to_radians()), "{s}");
}

#[test]
fn constraints_on_the_axes_and_the_origin_solve() {
    let mut d = with_sketch();
    let x = axis_id(&mut d, "x");
    let y = axis_id(&mut d, "y");
    let origin = axis_id(&mut d, "origin");

    // A point on the y axis.
    let p = add_point(&mut d, [7.0, 9.0]);
    constrain(
        &mut d,
        json!({"type": "coincident", "point": p, "entity": y}),
    );
    let s = sketch(&d);
    assert!(close(at(&s, &p)[0], 0.0), "{s}");
    assert!(close(at(&s, &p)[1], 9.0), "{s}");

    // Two points mirrored about the x axis.
    let a = add_point(&mut d, [20.0, 6.0]);
    let b = add_point(&mut d, [21.0, -4.0]);
    constrain(
        &mut d,
        json!({"type": "symmetric", "a": a, "b": b, "axis": x}),
    );
    let s = sketch(&d);
    let (pa, pb) = (at(&s, &a), at(&s, &b));
    assert!(close(pa[0], pb[0]) && close(pa[1], -pb[1]), "{s}");

    // A point level with the origin.
    let q = add_point(&mut d, [-15.0, 3.0]);
    constrain(
        &mut d,
        json!({"type": "horizontal_points", "a": origin, "b": q}),
    );
    assert!(close(at(&sketch(&d), &q)[1], 0.0));

    // A line parallel to the y axis, one perpendicular to it, one
    // collinear with the x axis.
    let (l1, s1, e1) = line(&mut d, [30.0, 0.0], [33.0, 20.0]);
    constrain(&mut d, json!({"type": "parallel", "a": y, "b": l1}));
    let (l2, s2, e2) = line(&mut d, [-30.0, 10.0], [-10.0, 14.0]);
    constrain(&mut d, json!({"type": "perpendicular", "a": y, "b": l2}));
    let (l3, s3, e3) = line(&mut d, [40.0, 2.0], [60.0, 3.0]);
    constrain(&mut d, json!({"type": "collinear", "a": x, "b": l3}));
    // A line with its midpoint at the origin.
    let (l4, s4, e4) = line(&mut d, [-12.0, -20.0], [8.0, -25.0]);
    constrain(
        &mut d,
        json!({"type": "midpoint", "point": origin, "curve": l4}),
    );
    // A circle tangent to the x axis.
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "center",
               "center": [-40, 12], "radius": 5, "construction": false}),
    );
    let c = circle["made"][0].as_str().unwrap().to_owned();
    constrain(&mut d, json!({"type": "tangent", "a": x, "b": c}));

    let s = sketch(&d);
    assert!(s["error"].is_null(), "{s}");
    assert!(close(at(&s, &s1)[0], at(&s, &e1)[0]), "{l1}: {s}");
    assert!(close(at(&s, &s2)[1], at(&s, &e2)[1]), "{l2}: {s}");
    for p in [&s3, &e3] {
        assert!(close(at(&s, p)[1], 0.0), "{l3}: {s}");
    }
    let (m4, n4) = (at(&s, &s4), at(&s, &e4));
    assert!(
        close(m4[0] + n4[0], 0.0) && close(m4[1] + n4[1], 0.0),
        "{l4}: {s}"
    );
    let ce = entity(&s, &c);
    let centre = at(&s, ce["center"].as_str().unwrap());
    let radius = ce["radius"].as_f64().unwrap();
    assert!(close(centre[1].abs(), radius), "{c}: {s}");
    // The axes stayed where they are.
    let [ox, oy] = at(&s, &origin);
    assert!(close(ox, 0.0) && close(oy, 0.0), "{s}");
    let x_end = entity(&s, &x)["end"].as_str().unwrap().to_owned();
    assert!(close(at(&s, &x_end)[1], 0.0), "{s}");
}

#[test]
fn undo_takes_an_axis_back_with_the_step_that_made_it() {
    let mut d = with_sketch();
    let p = add_point(&mut d, [5.0, 8.0]);
    let depth = d.undo_depth();
    // As the app does it: the axis made on demand and the constraint, one
    // step.
    let y = axis_id(&mut d, "y");
    constrain(
        &mut d,
        json!({"type": "coincident", "point": p, "entity": y}),
    );
    command(
        &mut d,
        json!({"cmd": "merge_undo", "depth": depth, "label": "Add Constraint to Sketch1"}),
    );
    assert_eq!(d.undo_depth(), depth + 1);
    assert_eq!(sketch(&d)["entities"].as_array().unwrap().len(), 4);
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d);
    let ids: Vec<&str> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![p.as_str()], "{s}");
    assert!(s["constraints"].as_array().unwrap().is_empty(), "{s}");
    // Made again on the next pick.
    assert_eq!(axis(&mut d, "y")["new"], true);
}

#[test]
fn the_axes_bound_no_profile_and_are_not_exported() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point",
               "a": [-10, -10], "b": [10, 10]}),
    );
    axis_id(&mut d, "x");
    axis_id(&mut d, "y");
    let s = sketch(&d);
    let areas: Vec<f64> = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["area"].as_f64().unwrap())
        .collect();
    assert_eq!(areas.len(), 1, "{s}");
    assert!(close(areas[0], 400.0), "{s}");
}

#[test]
fn the_axis_flag_reads_and_writes_and_sketches_without_it_are_as_before() {
    let flagged = json!({"id": "c3", "type": "line", "start": "p1", "end": "p2",
                         "construction": true, "fixed": true, "axis": "x"});
    let e: Entity = serde_json::from_value(flagged.clone()).unwrap();
    assert_eq!(e.axis, Some(SketchAxis::X));
    assert_eq!(serde_json::to_value(&e).unwrap(), flagged);
    let point =
        json!({"id": "p2", "type": "point", "at": [0.0, 100.0], "fixed": true, "axis": "y"});
    let e: Entity = serde_json::from_value(point.clone()).unwrap();
    assert_eq!(e.axis, Some(SketchAxis::Y));
    assert_eq!(serde_json::to_value(&e).unwrap(), point);
    // Saved before: no flag, written back without one.
    let plain = json!({"id": "c3", "type": "line", "start": "p1", "end": "p2"});
    let e: Entity = serde_json::from_value(plain.clone()).unwrap();
    assert_eq!(e.axis, None);
    assert_eq!(serde_json::to_value(&e).unwrap(), plain);
    let error = serde_json::from_value::<Entity>(
        json!({"id": "c3", "type": "line", "start": "p1", "end": "p2", "axis": "z"}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("\"axis\" must be"), "{error}");
}

/// The sketch solves without redundant or conflicting constraints, its
/// axis lines fixed and fully constrained.
fn assert_axes_clean(s: &Value, axes: &[&str]) {
    assert!(s["error"].is_null(), "{s}");
    assert_eq!(s["redundant"], json!([]), "{s}");
    assert!(
        s["conflicts"].as_array().is_none_or(|c| c.is_empty()),
        "{s}"
    );
    let fully = s["fully_constrained"].as_array().unwrap();
    for id in axes {
        assert_eq!(entity(s, id)["fixed"], true, "{id}: {s}");
        assert!(fully.contains(&json!(id)), "{id}: {s}");
    }
}

#[test]
fn an_axis_line_is_not_redundant_alone_or_constrained() {
    // The x axis alone: its fix repeats its fixed end points, which hold
    // it, so it is not reported redundant (mitcad#877).
    let mut d = with_sketch();
    let x = axis_id(&mut d, "x");
    let s = sketch(&d);
    assert_eq!(s["dof"], 0, "{s}");
    assert_axes_clean(&s, &[x.as_str()]);

    // With a point on it.
    let p = add_point(&mut d, [12.0, 5.0]);
    constrain(
        &mut d,
        json!({"type": "coincident", "point": p, "entity": x}),
    );
    let s = sketch(&d);
    assert!(close(at(&s, &p)[1], 0.0), "{s}");
    assert_axes_clean(&s, &[x.as_str()]);

    // Two points symmetric about it.
    let a = add_point(&mut d, [20.0, 6.0]);
    let b = add_point(&mut d, [21.0, -4.0]);
    constrain(
        &mut d,
        json!({"type": "symmetric", "a": a, "b": b, "axis": x}),
    );
    let s = sketch(&d);
    let (pa, pb) = (at(&s, &a), at(&s, &b));
    assert!(close(pa[0], pb[0]) && close(pa[1], -pb[1]), "{s}");
    assert_axes_clean(&s, &[x.as_str()]);

    // The y axis beside it, sharing the origin, and a line parallel to it.
    let y = axis_id(&mut d, "y");
    let (l, _, _) = line(&mut d, [30.0, 0.0], [33.0, 20.0]);
    constrain(&mut d, json!({"type": "parallel", "a": y, "b": l}));
    let s = sketch(&d);
    assert_axes_clean(&s, &[x.as_str(), y.as_str()]);
}
