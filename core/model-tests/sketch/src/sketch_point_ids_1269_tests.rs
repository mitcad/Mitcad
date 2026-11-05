// SPDX-License-Identifier: MIT
//! Existing points as the drawing tools' point inputs (mitcad#1269): a
//! polygon centred on a point is centred on that point, with no point of
//! its own; a rectangle's corner or centre, a slot's centres, an ellipse's
//! major axis end are the points given; a circle or an arc through an
//! existing point holds it. Fields that are only positions refuse a point
//! id or say so; a point id that names a curve says how ids are numbered.
//! And the `profiles` query of one sketch.

use std::f64::consts::PI;

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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "sketch", "uid": "F1"}))
}

/// Adds points at these places: p1, p2, ... in an empty sketch.
fn points_at(doc: &mut Document<MockKernel>, places: &[[f64; 2]]) {
    for (i, at) in places.iter().enumerate() {
        let added = command(
            doc,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": at}),
        );
        assert_eq!(added["entities"], json!([format!("p{}", i + 1)]));
    }
}

fn entities_of(s: &Value, kind: &str) -> Vec<Value> {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == kind)
        .cloned()
        .collect()
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

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    close(a[0], b[0]) && close(a[1], b[1])
}

fn areas(s: &Value) -> Vec<f64> {
    s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["area"].as_f64().unwrap())
        .collect()
}

fn assert_one_area(s: &Value, expected: f64) {
    let areas = areas(s);
    assert_eq!(areas.len(), 1, "{areas:?}");
    assert!(close(areas[0], expected), "{areas:?} != {expected}");
}

/// The lines that end at a point, of the outline (not construction) or of
/// construction.
fn lines_at_of(s: &Value, point: &str, construction: bool) -> usize {
    entities_of(s, "line")
        .iter()
        .filter(|l| (l["construction"] == true) == construction)
        .filter(|l| l["start"] == point || l["end"] == point)
        .count()
}

/// The outline's lines that end at a point (a centre rectangle's
/// construction diagonal ends at two corners too).
fn lines_at(s: &Value, point: &str) -> usize {
    lines_at_of(s, point, false)
}

/// The constraints of a type.
fn constraints_of(s: &Value, kind: &str) -> Vec<Value> {
    s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == kind)
        .cloned()
        .collect()
}

#[test]
fn a_polygon_centred_on_an_existing_point_has_no_centre_of_its_own() {
    // The chat agent's case: `center: "p1"` was refused as not [x, y].
    for inscribed in [true, false] {
        let mut d = with_sketch();
        points_at(&mut d, &[[5.0, 3.0]]);
        let polygon = command(
            &mut d,
            json!({"cmd": "sketch.polygon", "sketch": "F1", "center": "p1", "vertex": [15, 3],
                   "sides": 6, "inscribed": inscribed}),
        );
        assert_eq!(
            polygon["made"],
            json!(["c2", "c3", "c4", "c5", "c6", "c7"]),
            "{inscribed}"
        );
        // Its placement, orientation and size, p1's place among them.
        assert_eq!(polygon["status"]["dof"], 4, "{inscribed}");
        let s = sketch(&d);
        // The six corners and p1: no other point.
        assert_eq!(entities_of(&s, "point").len(), 7, "{inscribed}");
        assert!(
            !polygon["entities"]
                .as_array()
                .unwrap()
                .contains(&json!("p1"))
        );
        let circles = entities_of(&s, "circle");
        assert_eq!(circles.len(), if inscribed { 1 } else { 2 });
        assert!(circles.iter().all(|c| c["center"] == "p1"), "{circles:?}");
        assert!(near(at(&s, "p1"), [5.0, 3.0]));
        // The corners around p1: on the circle through the vertex, or with
        // the vertex at a side's middle.
        let corner = if inscribed {
            10.0
        } else {
            10.0 / (PI / 6.0).cos()
        };
        for p in entities_of(&s, "point") {
            let id = p["id"].as_str().unwrap();
            if id != "p1" {
                let q = at(&s, id);
                assert!(close((q[0] - 5.0).hypot(q[1] - 3.0), corner), "{id} {q:?}");
            }
        }
        let area = if inscribed {
            3.0 * 3f64.sqrt() / 2.0 * 100.0
        } else {
            6.0 * 100.0 * (PI / 6.0).tan()
        };
        assert_one_area(&s, area);
        // It follows p1, regular around it.
        command(
            &mut d,
            json!({"cmd": "sketch.drag", "sketch": "F1", "entity": "p1", "to": [25, 3]}),
        );
        let s = sketch(&d);
        let centre = at(&s, "p1");
        assert!(near(centre, [25.0, 3.0]), "{inscribed} {centre:?}");
        let distances: Vec<f64> = entities_of(&s, "point")
            .iter()
            .map(|p| p["id"].as_str().unwrap().to_owned())
            .filter(|id| id != "p1")
            .map(|id| {
                let q = at(&s, &id);
                (q[0] - centre[0]).hypot(q[1] - centre[1])
            })
            .collect();
        assert!(
            distances.iter().all(|r| close(*r, distances[0])),
            "{distances:?}"
        );
    }
}

#[test]
fn an_inscribed_polygon_s_vertex_may_be_an_existing_corner() {
    let mut d = with_sketch();
    points_at(&mut d, &[[0.0, 0.0], [10.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.polygon", "sketch": "F1", "center": "p1", "vertex": "p2", "sides": 4}),
    );
    let s = sketch(&d);
    // p1, p2 and three new corners.
    assert_eq!(entities_of(&s, "point").len(), 5);
    assert_eq!(lines_at(&s, "p2"), 2);
    assert!(near(at(&s, "p2"), [10.0, 0.0]));
    assert_one_area(&s, 200.0);
    // A circumscribed polygon's vertex is a side's middle: a position only.
    let mut d = with_sketch();
    points_at(&mut d, &[[0.0, 0.0], [10.0, 0.0]]);
    let refusal = rejected(
        &mut d,
        json!({"cmd": "sketch.polygon", "sketch": "F1", "center": "p1", "vertex": "p2",
               "sides": 4, "inscribed": false}),
    );
    assert!(refusal.contains("middle of a side"), "{refusal}");
}

#[test]
fn rectangles_take_existing_corners_and_centres() {
    // Two corners, both existing: they are two of the four corners.
    let mut d = with_sketch();
    points_at(&mut d, &[[20.0, 10.0], [0.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point", "a": "p1", "b": "p2"}),
    );
    let s = sketch(&d);
    assert_eq!(entities_of(&s, "point").len(), 4);
    assert_eq!(lines_at(&s, "p1"), 2);
    assert_eq!(lines_at(&s, "p2"), 2);
    assert_one_area(&s, 200.0);
    // One existing corner and one position.
    let mut d = with_sketch();
    points_at(&mut d, &[[0.0, 10.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "two_point", "a": "p1", "b": [20, 0]}),
    );
    let s = sketch(&d);
    assert_eq!(entities_of(&s, "point").len(), 4);
    assert_eq!(lines_at(&s, "p1"), 2);
    assert_one_area(&s, 200.0);
    // The centre of a centre rectangle is the existing point, held at its
    // diagonal's middle.
    let mut d = with_sketch();
    points_at(&mut d, &[[5.0, 5.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "center", "center": "p1",
               "corner": [15, 10]}),
    );
    let s = sketch(&d);
    assert_eq!(entities_of(&s, "point").len(), 5);
    let middles = constraints_of(&s, "midpoint");
    assert_eq!(middles.len(), 1);
    assert_eq!(middles[0]["point"], "p1");
    assert_one_area(&s, 200.0);
    // An existing corner of a centre rectangle.
    let mut d = with_sketch();
    points_at(&mut d, &[[15.0, 10.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "center", "center": [5, 5],
               "corner": "p1"}),
    );
    let s = sketch(&d);
    assert_eq!(lines_at(&s, "p1"), 2);
    // The upper right corner is an end of the construction diagonal too.
    assert_eq!(lines_at_of(&s, "p1", true), 1);
    assert_one_area(&s, 200.0);
    // Three points: the first side's ends; c is a position only.
    let mut d = with_sketch();
    points_at(&mut d, &[[0.0, 0.0], [20.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "three_point", "a": "p1",
               "b": "p2", "c": [20, 10]}),
    );
    let s = sketch(&d);
    assert_eq!(entities_of(&s, "point").len(), 4);
    assert_eq!(lines_at(&s, "p1"), 2);
    assert_eq!(lines_at(&s, "p2"), 2);
    assert_one_area(&s, 200.0);
    let mut d = with_sketch();
    points_at(&mut d, &[[0.0, 0.0], [20.0, 0.0], [20.0, 10.0]]);
    rejected(
        &mut d,
        json!({"cmd": "sketch.rectangle", "sketch": "F1", "mode": "three_point", "a": "p1",
               "b": "p2", "c": "p3"}),
    );
}

#[test]
fn circles_and_arcs_through_existing_points_hold_them() {
    // Two points: the ends of a diameter, both held on the circle.
    let mut d = with_sketch();
    points_at(&mut d, &[[0.0, 0.0], [10.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "two_point", "a": "p1", "b": "p2"}),
    );
    let s = sketch(&d);
    let circle = &entities_of(&s, "circle")[0];
    assert!(close(circle["geometry"]["radius"].as_f64().unwrap(), 5.0));
    let held = constraints_of(&s, "coincident");
    assert_eq!(held.len(), 2, "{held:?}");
    assert!(held.iter().all(|c| c["entity"] == circle["id"]));
    // The centre and p1, p2: no point at either.
    assert_eq!(entities_of(&s, "point").len(), 3);
    // Three points, one of them a position.
    let mut d = with_sketch();
    points_at(&mut d, &[[10.0, 0.0], [-10.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F1", "mode": "three_point", "a": "p1",
               "b": [0, 10], "c": "p2"}),
    );
    let s = sketch(&d);
    let circle = &entities_of(&s, "circle")[0];
    assert!(close(circle["geometry"]["radius"].as_f64().unwrap(), 10.0));
    assert_eq!(constraints_of(&s, "coincident").len(), 2);
    // An arc through an existing point holds it; its ends are the points
    // given.
    let mut d = with_sketch();
    points_at(&mut d, &[[10.0, 0.0], [-10.0, 0.0], [0.0, 10.0]]);
    let arc = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point", "start": "p1",
               "through": "p3", "end": "p2"}),
    );
    let s = sketch(&d);
    let id = arc["made"][0].as_str().unwrap();
    let held = constraints_of(&s, "coincident");
    assert_eq!(held.len(), 1);
    assert_eq!(held[0]["point"], "p3");
    assert_eq!(held[0]["entity"], id);
    let arc = entity(&s, id);
    let ends = [arc["start"].clone(), arc["end"].clone()];
    assert!(ends.contains(&json!("p1")) && ends.contains(&json!("p2")));
    assert!(close(arc["geometry"]["radius"].as_f64().unwrap(), 10.0));
}

#[test]
fn slots_take_existing_centres() {
    // Between two existing centres.
    let mut d = with_sketch();
    points_at(&mut d, &[[0.0, 0.0], [30.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.slot", "sketch": "F1", "mode": "center_to_center", "a": "p1",
               "b": "p2", "width": 10}),
    );
    let s = sketch(&d);
    let centres: Vec<Value> = entities_of(&s, "arc")
        .iter()
        .map(|a| a["center"].clone())
        .collect();
    assert_eq!(centres.len(), 2);
    assert!(centres.contains(&json!("p1")) && centres.contains(&json!("p2")));
    // p1, p2 and the four side ends.
    assert_eq!(entities_of(&s, "point").len(), 6);
    assert_one_area(&s, 300.0 + PI * 25.0);
    // The slot's middle and an end's centre.
    let mut d = with_sketch();
    points_at(&mut d, &[[0.0, 0.0], [15.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.slot", "sketch": "F1", "mode": "center_point", "center": "p1",
               "end": "p2", "width": 10}),
    );
    let s = sketch(&d);
    let middles = constraints_of(&s, "midpoint");
    assert_eq!(middles.len(), 1);
    assert_eq!(middles[0]["point"], "p1");
    assert!(entities_of(&s, "arc").iter().any(|a| a["center"] == "p2"));
    assert_one_area(&s, 300.0 + PI * 25.0);
    // An arc slot around an existing centre from an existing start; its
    // end is a direction only.
    let mut d = with_sketch();
    points_at(&mut d, &[[0.0, 0.0], [50.0, 0.0], [0.0, 50.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.slot", "sketch": "F1", "mode": "arc", "center": "p1",
               "start": "p2", "end": [0, 50], "width": 10}),
    );
    let s = sketch(&d);
    let arcs = entities_of(&s, "arc");
    assert_eq!(arcs.iter().filter(|a| a["center"] == "p1").count(), 3);
    assert!(arcs.iter().any(|a| a["center"] == "p2"));
    let sector = PI / 4.0 * (55.0f64.powi(2) - 45.0f64.powi(2));
    assert_one_area(&s, sector + PI * 25.0);
    let refusal = rejected(
        &mut d,
        json!({"cmd": "sketch.slot", "sketch": "F1", "mode": "arc", "center": "p1",
               "start": "p2", "end": "p3", "width": 10}),
    );
    assert!(refusal.contains("direction"), "{refusal}");
}

#[test]
fn an_ellipse_s_major_axis_end_may_be_an_existing_point() {
    let mut d = with_sketch();
    points_at(&mut d, &[[0.0, 0.0], [20.0, 0.0]]);
    command(
        &mut d,
        json!({"cmd": "sketch.ellipse", "sketch": "F1", "center": "p1", "major": "p2",
               "minor_radius": 5}),
    );
    let s = sketch(&d);
    let ellipse = &entities_of(&s, "ellipse")[0];
    assert_eq!(ellipse["center"], "p1");
    assert_eq!(ellipse["major"], "p2");
    assert_eq!(entities_of(&s, "point").len(), 2);
    assert_one_area(&s, PI * 20.0 * 5.0);
    let refusal = rejected(
        &mut d,
        json!({"cmd": "sketch.ellipse", "sketch": "F1", "center": "p1", "major": "p1",
               "minor_radius": 5}),
    );
    assert!(refusal.contains("major axis"), "{refusal}");
}

#[test]
fn a_point_id_that_names_a_curve_says_how_ids_are_numbered() {
    // A line made c1, p2 and p3: "p1" is the line, not a point.
    let mut d = with_sketch();
    let line = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]}),
    );
    assert_eq!(line["entities"], json!(["p2", "p3", "c1"]));
    let refusal = rejected(
        &mut d,
        json!({"cmd": "sketch.polygon", "sketch": "F1", "center": "p1", "vertex": [10, 10],
               "sides": 3}),
    );
    assert!(refusal.contains("c1 is not a point"), "{refusal}");
    assert!(refusal.contains("share one numbering"), "{refusal}");
    let refusal = rejected(
        &mut d,
        json!({"cmd": "sketch.polygon", "sketch": "F1", "center": "p9", "vertex": [10, 10],
               "sides": 3}),
    );
    assert!(refusal.contains("point p9 does not exist"), "{refusal}");
    // A point input [x, y] is a new point, also where a curve ends.
    let next = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [10, 0], "end": [10, 10]}),
    );
    assert_eq!(next["entities"], json!(["p5", "p6", "c4"]));
    let joined = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p3", "end": [0, 10]}),
    );
    assert_eq!(joined["entities"], json!(["p8", "c7"]));
}

#[test]
fn profiles_of_one_sketch() {
    let mut d = with_sketch();
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10,
               "height": 10}),
    );
    command(&mut d, json!({"cmd": "sketch.create", "name": "Plate"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F2", "center": [50, 0], "diameter": 10}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": "F2", "center": [80, 0], "diameter": 10}),
    );
    let all = query(&d, json!({"query": "profiles"}));
    assert_eq!(all.as_array().unwrap().len(), 3, "{all}");
    let sketches = |profiles: &Value| -> Vec<String> {
        profiles
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["sketch"].as_str().unwrap().to_owned())
            .collect()
    };
    let first = query(&d, json!({"query": "profiles", "sketch": "F1"}));
    assert_eq!(sketches(&first), ["F1"]);
    let by_name = query(&d, json!({"query": "profiles", "sketch": "Plate"}));
    assert_eq!(sketches(&by_name), ["F2", "F2"]);
    assert_eq!(by_name[0]["sketch_name"], "Plate");
    let hashed = query(
        &d,
        json!({"query": "profiles", "sketch": "Sketch1", "hashes": true}),
    );
    assert_eq!(sketches(&hashed), ["F1"]);
    assert!(hashed[0]["hash"].is_string());
    for unknown in ["F9", "Nothing"] {
        let refusal = d
            .query(&json!({"query": "profiles", "sketch": unknown}).to_string())
            .unwrap_err()
            .to_string();
        assert!(
            refusal.contains(&format!("no sketch {unknown}")),
            "{refusal}"
        );
    }
}
