// SPDX-License-Identifier: MIT
//! Regular polygons stay regular (mitcad#303): the constraints of
//! `sketch.polygon` keep equal sides and equal angles while a corner is
//! dragged, the polygon is turned with Move, translated or resized, after
//! undo and redo and in a saved and reopened design, with the mock kernel.

use std::f64::consts::{PI, TAU};

use mitcad_vec3::vec2::distance as dist;
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

/// A polygon in a new sketch; returns the document and its sides in order.
fn polygon(
    sides: usize,
    inscribed: bool,
    center: [f64; 2],
    vertex: [f64; 2],
) -> (Document<MockKernel>, Vec<String>) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let r = command(
        &mut d,
        json!({"cmd": "sketch.polygon", "sketch": "F1", "center": center, "vertex": vertex,
               "sides": sides, "inscribed": inscribed}),
    );
    let lines: Vec<String> = r["made"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(lines.len(), sides, "{r}");
    (d, lines)
}

/// The corners in order: each side's start.
fn corners(d: &Document<MockKernel>, lines: &[String]) -> Vec<[f64; 2]> {
    let s = sketch(d);
    lines
        .iter()
        .map(|l| xy(&entity(&s, l)["geometry"]["start"]))
        .collect()
}

/// The polygon's corner id of side `i` (its start point).
fn corner_id(d: &Document<MockKernel>, line: &str) -> String {
    let s = sketch(d);
    let e = entity(&s, line);
    e["start"]
        .as_str()
        .unwrap_or_else(|| panic!("no start in {e}"))
        .to_owned()
}

/// Centre and side length of a regular polygon; panics if the corners are
/// not one (sides, corner radii and turning angles all equal).
fn regular(p: &[[f64; 2]]) -> ([f64; 2], f64) {
    let n = p.len();
    let c = [
        p.iter().map(|q| q[0]).sum::<f64>() / n as f64,
        p.iter().map(|q| q[1]).sum::<f64>() / n as f64,
    ];
    let side = dist(p[0], p[1]);
    let radius = dist(c, p[0]);
    let tol = 1e-6 * radius.max(1.0);
    for i in 0..n {
        let (a, b, e) = (p[i], p[(i + 1) % n], p[(i + 2) % n]);
        assert!(
            (dist(a, b) - side).abs() <= tol,
            "side {i}: {} != {side} in {p:?}",
            dist(a, b)
        );
        assert!(
            (dist(c, a) - radius).abs() <= tol,
            "corner {i} not on the circumcircle in {p:?}"
        );
        // The turn at each corner is a full turn over n, all the same way.
        let (u, v) = ([b[0] - a[0], b[1] - a[1]], [e[0] - b[0], e[1] - b[1]]);
        let turn = (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1]);
        assert!(
            (turn - TAU / n as f64).abs() <= 1e-6,
            "turn at corner {}: {turn} in {p:?}",
            (i + 1) % n
        );
    }
    (c, side)
}

/// Every way a polygon is made: inscribed, circumscribed, and an edge
/// polygon (what the Edge Polygon tool sends: the centre beside an edge and
/// the edge's start as an inscribed corner).
fn kinds(sides: usize) -> Vec<(&'static str, bool, [f64; 2], [f64; 2])> {
    let (a, b) = ([5.0, -3.0], [17.0, 2.0]);
    let length = dist(a, b);
    let normal = [-(b[1] - a[1]) / length, (b[0] - a[0]) / length];
    let h = length / 2.0 / (PI / sides as f64).tan();
    let edge_center = [
        (a[0] + b[0]) / 2.0 + normal[0] * h,
        (a[1] + b[1]) / 2.0 + normal[1] * h,
    ];
    vec![
        ("inscribed", true, [3.0, 4.0], [13.0, 6.0]),
        ("circumscribed", false, [3.0, 4.0], [13.0, 6.0]),
        ("edge", true, edge_center, a),
    ]
}

#[test]
fn new_polygons_are_regular_with_four_degrees_of_freedom() {
    for sides in [3, 4, 5, 6, 7, 8, 12] {
        for (kind, inscribed, center, vertex) in kinds(sides) {
            let (d, lines) = polygon(sides, inscribed, center, vertex);
            let s = sketch(&d);
            // Placement (2), orientation (1) and size (1) stay free, no more.
            assert_eq!(s["dof"], 4, "{kind} {sides}: {}", s["status"]);
            let (c, _) = regular(&corners(&d, &lines));
            assert!(dist(c, center) < 1e-6, "{kind} {sides}");
        }
    }
}

#[test]
fn dragging_a_corner_keeps_the_polygon_regular() {
    for sides in [5, 6] {
        for (kind, inscribed, center, vertex) in kinds(sides) {
            eprintln!("{kind} {sides}");
            let (mut d, lines) = polygon(sides, inscribed, center, vertex);
            let p = corner_id(&d, &lines[0]);
            // Pull a corner around the centre in steps, as the mouse does.
            let start = corners(&d, &lines)[0];
            let r0 = dist(center, start);
            let a0 = (start[1] - center[1]).atan2(start[0] - center[0]);
            for step in 1..=12 {
                let a = a0 + 0.1 * f64::from(step);
                let to = [center[0] + r0 * a.cos(), center[1] + r0 * a.sin()];
                command(
                    &mut d,
                    json!({"cmd": "sketch.drag", "sketch": "F1", "entity": p, "to": to}),
                );
                let now = corners(&d, &lines);
                assert!(
                    dist(now[0], to) < 1e-6,
                    "{kind} {sides}: the corner did not follow"
                );
                regular(&now);
            }
            // And a pull off the circle: still regular.
            command(
                &mut d,
                json!({"cmd": "sketch.drag", "sketch": "F1", "entity": p, "to": [40, -25]}),
            );
            regular(&corners(&d, &lines));
        }
    }
}

#[test]
fn a_fixed_centre_and_size_make_a_corner_drag_a_pure_rotation() {
    for sides in [5, 6] {
        for (kind, inscribed, center, vertex) in kinds(sides) {
            eprintln!("{kind} {sides}");
            let (mut d, lines) = polygon(sides, inscribed, center, vertex);
            let s = sketch(&d);
            let circle = s["entities"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["type"] == "circle")
                .unwrap()
                .clone();
            let circle_id = circle["id"].as_str().unwrap().to_owned();
            let centre_id = circle["center"].as_str().unwrap().to_owned();
            command(
                &mut d,
                json!({"cmd": "sketch.set_fixed", "sketch": "F1", "entities": [centre_id]}),
            );
            let radius = circle["geometry"]["radius"].as_f64().unwrap();
            command(
                &mut d,
                json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                       "dimension": {"type": "radius", "curve": circle_id}, "value": radius}),
            );
            assert_eq!(
                sketch(&d)["dof"],
                1,
                "{kind} {sides}: only the orientation is free"
            );
            let side = regular(&corners(&d, &lines)).1;
            let p = corner_id(&d, &lines[0]);
            command(
                &mut d,
                json!({"cmd": "sketch.drag", "sketch": "F1", "entity": p, "to": [center[0] - 50.0, center[1] + 50.0]}),
            );
            let now = corners(&d, &lines);
            let (c, side_now) = regular(&now);
            assert!(dist(c, center) < 1e-6, "{kind} {sides}: the center moved");
            assert!(
                (side_now - side).abs() < 1e-6,
                "{kind} {sides}: the size changed"
            );
            // The corner turned toward the pointer.
            let a = (now[0][1] - center[1]).atan2(now[0][0] - center[0]);
            assert!((a - 3.0 * PI / 4.0).abs() < 1e-6, "{kind} {sides}: {a}");
            // Rotate about the fixed centre: a pure turn by the angle.
            command(
                &mut d,
                json!({"cmd": "sketch.move", "sketch": "F1", "entities": lines,
                       "rotate": {"center": center, "angle": 0.4}}),
            );
            let turned = corners(&d, &lines);
            let (c, side_now) = regular(&turned);
            assert!(
                dist(c, center) < 1e-6 && (side_now - side).abs() < 1e-6,
                "{kind} {sides}"
            );
            let b = (turned[0][1] - center[1]).atan2(turned[0][0] - center[0]);
            assert!(
                (b - a - 0.4).abs() < 1e-6,
                "{kind} {sides}: turned by {}",
                b - a
            );
            // Rotate about another point cannot move the fixed centre: the
            // polygon goes as near as its constraints let it, still
            // regular, the same size and in the same place.
            command(
                &mut d,
                json!({"cmd": "sketch.move", "sketch": "F1", "entities": lines,
                       "rotate": {"center": [center[0] + 30.0, center[1]], "angle": 1.0}}),
            );
            let (c, side_now) = regular(&corners(&d, &lines));
            assert!(
                dist(c, center) < 1e-6 && (side_now - side).abs() < 1e-6,
                "{kind} {sides}"
            );
        }
    }
}

#[test]
fn move_turns_translates_and_a_radius_drag_resizes_uniformly() {
    for sides in [5, 6] {
        for (kind, inscribed, center, vertex) in kinds(sides) {
            eprintln!("{kind} {sides}");
            let (mut d, lines) = polygon(sides, inscribed, center, vertex);
            let side = regular(&corners(&d, &lines)).1;
            // Rotate (Move with a turn) of the sides only, about a point
            // that is not the centre.
            command(
                &mut d,
                json!({"cmd": "sketch.move", "sketch": "F1", "entities": lines,
                       "rotate": {"center": [-7, 2], "angle": 0.7}}),
            );
            let (_, side_now) = regular(&corners(&d, &lines));
            assert!(
                (side_now - side).abs() < 1e-6,
                "{kind} {sides}: Rotate changed the size"
            );
            // A translation by dragging a side.
            let before = corners(&d, &lines);
            command(
                &mut d,
                json!({"cmd": "sketch.drag", "sketch": "F1", "entity": lines[2], "by": [3, -8]}),
            );
            let after = corners(&d, &lines);
            for (a, b) in before.iter().zip(&after) {
                assert!(
                    dist([a[0] + 3.0, a[1] - 8.0], *b) < 1e-6,
                    "{kind} {sides}: not a translation"
                );
            }
            // A deliberate resize: the construction circle's radius.
            let s = sketch(&d);
            let circle = s["entities"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["type"] == "circle")
                .unwrap()
                .clone();
            let r = circle["geometry"]["radius"].as_f64().unwrap();
            command(
                &mut d,
                json!({"cmd": "sketch.drag", "sketch": "F1", "entity": circle["id"], "radius": 2.0 * r}),
            );
            let (_, resized) = regular(&corners(&d, &lines));
            assert!(
                (resized - 2.0 * side).abs() < 1e-6,
                "{kind} {sides}: not uniform"
            );
        }
    }
}

#[test]
fn shape_constraints_survive_undo_redo_and_save() {
    let (mut d, lines) = polygon(6, false, [0.0, 0.0], [10.0, 0.0]);
    let p = corner_id(&d, &lines[0]);
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": p, "to": [3, 12]}),
    );
    command(&mut d, json!({"cmd": "undo"}));
    regular(&corners(&d, &lines));
    command(&mut d, json!({"cmd": "undo"}));
    command(&mut d, json!({"cmd": "redo"}));
    command(&mut d, json!({"cmd": "redo"}));
    regular(&corners(&d, &lines));
    command(
        &mut d,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": p, "to": [-9, 7]}),
    );
    regular(&corners(&d, &lines));
    let saved = d.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(sketch(&loaded)["dof"], 4);
    command(
        &mut loaded,
        json!({"cmd": "sketch.drag", "sketch": "F1", "entity": p, "to": [12, -2]}),
    );
    regular(&corners(&loaded, &lines));
}

#[test]
fn a_dimension_that_would_deform_the_polygon_is_refused() {
    for inscribed in [true, false] {
        let (mut d, lines) = polygon(6, inscribed, [0.0, 0.0], [10.0, 0.0]);
        command(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": {"type": "length", "line": lines[0]}, "value": 10}),
        );
        let error = rejected(
            &mut d,
            json!({"cmd": "sketch.add_dimension", "sketch": "F1",
                   "dimension": {"type": "length", "line": lines[3]}, "value": 14}),
        );
        assert!(error.contains("over-constrained"), "{error}");
        regular(&corners(&d, &lines));
    }
}
