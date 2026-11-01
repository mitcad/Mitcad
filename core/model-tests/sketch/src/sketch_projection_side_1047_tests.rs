// SPDX-License-Identifier: MIT
//! Geometry dimensioned to a linked projection keeps the side it has in the
//! stored sketch when the projection's source moves, whatever the size of
//! the move (mitcad#1047), with the mock kernel. A block's side edge is
//! projected into a sketch on its top face, over 80 mm long; a rectangle, a
//! circle or an offset is tied to the projected line by 0.5 mm. Every
//! regeneration solves from the stored positions, and a move of 5 mm in a
//! sketch that size is one solve, not steps: a point 0.5 mm from the line
//! that moved 0.5 mm toward it lay on the moved line at the solve's start,
//! and the side of its distance came out by chance (the other side), so
//! that the rectangle straddled the line and its region split.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": uid}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

/// The block's rectangle region in its sketch F1.
const BLOCK: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

/// The moves of the source toward the geometry tied to it (mm): less than,
/// as much as and more than the distance, and ten times it.
const MOVES: [f64; 4] = [0.25, 0.5, 0.75, 5.0];

/// The distance the geometry keeps from the projected line (mm).
const GAP: f64 = 0.5;

/// The block's side edge at x = `x` on its top face (z = 20), from y = -20
/// to 60: what the mock kernel gives for the projected face.
fn edge(x: f64) -> Vec<Curve3> {
    vec![Curve3::Line {
        start: [x, -20.0, 20.0],
        end: [x, 60.0, 20.0],
    }]
}

/// A 60 x 40 x 20 block (F2) whose width is the parameter d1, and the
/// sketch F3 on its top face with its side edge at x = 60 projected
/// (linked): the document and the projected line's id.
fn block_with_projected_edge() -> (Document<MockKernel>, String) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": BLOCK}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    let face = format!("F2:end({BLOCK})");
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": face}}),
    );
    *d.kernel().curves.borrow_mut() = edge(60.0);
    let projected = command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": face, "body": "F2.b0",
               "linked": true}),
    );
    let lines: Vec<String> = projected["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e.as_str())
        .filter(|e| e.starts_with('c'))
        .map(str::to_owned)
        .collect();
    assert_eq!(lines.len(), 1, "{projected}");
    (d, lines[0].clone())
}

/// The block grows by `by` toward the geometry: its side edge moves with
/// its width.
fn move_source(d: &mut Document<MockKernel>, by: f64) {
    *d.kernel().curves.borrow_mut() = edge(60.0 + by);
    let changed = command(
        d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 60.0 + by}),
    );
    assert_eq!(changed["error"], Value::Null, "{by}: {changed}");
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

/// The id of the point at a position in the sketch query.
fn point_id(s: &Value, at: [f64; 2]) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "point" && {
                let p = xy(&e["at"]);
                close(p[0], at[0]) && close(p[1], at[1])
            }
        })
        .unwrap_or_else(|| panic!("no point at {at:?} in {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The id of the line between two positions (either way round).
fn line_id(s: &Value, a: [f64; 2], b: [f64; 2]) -> String {
    let near = |p: [f64; 2], q: [f64; 2]| close(p[0], q[0]) && close(p[1], q[1]);
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "line" && {
                let (p, q) = (xy(&e["geometry"]["start"]), xy(&e["geometry"]["end"]));
                (near(p, a) && near(q, b)) || (near(p, b) && near(q, a))
            }
        })
        .unwrap_or_else(|| panic!("no line {a:?}-{b:?} in {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Both x of a line's ends.
fn line_x(s: &Value, id: &str) -> [f64; 2] {
    let g = &entity(s, id)["geometry"];
    [xy(&g["start"])[0], xy(&g["end"])[0]]
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

fn add_dimension(d: &mut Document<MockKernel>, dimension: Value, value: f64) -> String {
    let added = command(
        d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F3", "dimension": dimension,
               "value": value}),
    );
    added["dimensions"][0].as_str().unwrap().to_owned()
}

/// The regions of a sketch: key and area.
fn regions(s: &Value) -> Vec<(String, f64)> {
    s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["key"].as_str().unwrap().to_owned(),
                r["area"].as_f64().unwrap(),
            )
        })
        .collect()
}

/// The user's rectangle 10 x 20 beside the projected line, 0.5 mm from it
/// (x 60.5 to 70.5, y 10 to 30), with its width and height dimensioned:
/// its left and right lines.
fn rectangle(d: &mut Document<MockKernel>) -> (String, String) {
    command(
        d,
        json!({"cmd": "sketch.rectangle", "sketch": "F3", "mode": "two_point",
               "a": [60.5, 10], "b": [70.5, 30]}),
    );
    let s = sketch(d, "F3");
    let bottom = line_id(&s, [60.5, 10.0], [70.5, 10.0]);
    let left = line_id(&s, [60.5, 10.0], [60.5, 30.0]);
    let right = line_id(&s, [70.5, 10.0], [70.5, 30.0]);
    add_dimension(d, json!({"type": "length", "line": bottom}), 10.0);
    add_dimension(d, json!({"type": "length", "line": left}), 20.0);
    (left, right)
}

/// The rectangle's region (its area is 200).
fn rectangle_region(s: &Value) -> String {
    let found: Vec<String> = regions(s)
        .into_iter()
        .filter(|(_, area)| close(*area, 200.0))
        .map(|(key, _)| key)
        .collect();
    assert_eq!(found.len(), 1, "{s}");
    found[0].clone()
}

/// After each move the rectangle is beside the moved line on its side,
/// 0.5 mm from it, still one region of its own (the key it had).
fn check_rectangle(d: &mut Document<MockKernel>, (left, right): &(String, String), dim: &str) {
    let key = rectangle_region(&sketch(d, "F3"));
    for by in MOVES {
        move_source(d, by);
        let s = sketch(d, "F3");
        assert_eq!(s["error"], Value::Null, "{by}: {s}");
        let near = 60.0 + by + GAP;
        for (line, x) in [(left, near), (right, near + 10.0)] {
            let at = line_x(&s, line);
            assert!(
                close(at[0], x) && close(at[1], x),
                "{by}: {line} at {at:?}, not {x}"
            );
        }
        assert!(close(measured(&s, dim), GAP), "{by}: {s}");
        let found = regions(&s);
        assert!(
            found.len() == 1 && found[0].0 == key && close(found[0].1, 200.0),
            "{by}: {found:?}, not {key}"
        );
    }
}

#[test]
fn a_rectangle_a_point_line_distance_ties_to_a_projection_keeps_its_side() {
    let (mut d, projected) = block_with_projected_edge();
    let rectangle = rectangle(&mut d);
    let corner = point_id(&sketch(&d, "F3"), [60.5, 10.0]);
    let dim = add_dimension(
        &mut d,
        json!({"type": "point_line_distance", "point": corner, "line": projected}),
        GAP,
    );
    check_rectangle(&mut d, &rectangle, &dim);
}

#[test]
fn a_rectangle_a_line_distance_ties_to_a_projection_keeps_its_side() {
    let (mut d, projected) = block_with_projected_edge();
    let rectangle = rectangle(&mut d);
    let dim = add_dimension(
        &mut d,
        json!({"type": "line_distance", "a": projected, "b": rectangle.0}),
        GAP,
    );
    check_rectangle(&mut d, &rectangle, &dim);
}

#[test]
fn a_circle_a_tangent_distance_ties_to_a_projection_keeps_its_side() {
    // A circle of radius 2 whose centre is 2.5 from the line: a move of
    // 5 mm put the line past its centre before the solve.
    let (mut d, projected) = block_with_projected_edge();
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F3", "mode": "center",
               "center": [62.5, 20], "radius": 2}),
    );
    let circle = circle["made"][0].as_str().unwrap().to_owned();
    add_dimension(&mut d, json!({"type": "radius", "curve": circle}), 2.0);
    let dim = add_dimension(
        &mut d,
        json!({"type": "tangent_distance", "circle": circle, "line": projected}),
        GAP,
    );
    for by in MOVES {
        move_source(&mut d, by);
        let s = sketch(&d, "F3");
        assert_eq!(s["error"], Value::Null, "{by}: {s}");
        let g = &entity(&s, &circle)["geometry"];
        let centre = xy(&g["center"]);
        assert!(close(centre[0], 62.5 + by), "{by}: {centre:?}");
        assert!(close(g["radius"].as_f64().unwrap(), 2.0), "{by}: {g}");
        assert!(close(measured(&s, &dim), GAP), "{by}: {s}");
    }
}

#[test]
fn an_offset_of_a_projection_keeps_its_side_when_the_source_moves_by_its_distance() {
    let (mut d, projected) = block_with_projected_edge();
    command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": [projected], "distance": GAP}),
    );
    // The offset line: the one line besides the projection.
    let s = sketch(&d, "F3");
    let offset: Vec<String> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "line" && e["id"] != projected.as_str())
        .map(|e| e["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(offset.len(), 1, "{s}");
    let offset = &offset[0];
    // Its side as stored.
    let side = line_x(&s, offset)[0] - 60.0;
    assert!(close(side.abs(), GAP), "{s}");
    for by in MOVES {
        move_source(&mut d, by);
        let s = sketch(&d, "F3");
        assert_eq!(s["error"], Value::Null, "{by}: {s}");
        let x = 60.0 + by + side;
        let at = line_x(&s, offset);
        assert!(close(at[0], x) && close(at[1], x), "{by}: {at:?}, not {x}");
    }
}

/// The segment of `curve` in a region key, `c8[c7,c5]`.
fn segment(region: &str, curve: &str) -> String {
    let inner = region
        .strip_prefix("r{")
        .and_then(|r| r.strip_suffix('}'))
        .unwrap_or_else(|| panic!("{region}"));
    let head = format!("{curve}[");
    inner
        .split("],")
        .map(|piece| piece.trim_end_matches(']'))
        .find(|piece| piece.starts_with(&head))
        .map(|piece| format!("{piece}]"))
        .unwrap_or_else(|| panic!("no {curve} in {region}"))
}

fn status(d: &Document<MockKernel>, uid: &str) -> FeatureStatus {
    let uid: FeatureUid = uid.parse().unwrap();
    d.status(uid).cloned().expect("recomputed")
}

#[test]
fn a_pad_on_a_profile_tied_to_a_projection_and_an_extrude_up_to_its_side_stay_valid() {
    // As the design that showed it: the pad's profile is the rectangle
    // tied to the projected edge by 0.5 mm, a later extrusion goes up to
    // the pad's side face toward the edge, and the block's width grows by
    // 0.5 mm. Had the rectangle flipped over the edge, the edge would
    // split its region and that side face would be gone.
    let (mut d, projected) = block_with_projected_edge();
    let (left, _) = rectangle(&mut d);
    let corner = point_id(&sketch(&d, "F3"), [60.5, 10.0]);
    add_dimension(
        &mut d,
        json!({"type": "point_line_distance", "point": corner, "line": projected}),
        GAP,
    );
    let region = rectangle_region(&sketch(&d, "F3"));
    let pad = command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": region}],
               "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}),
    );
    let pad = pad["uid"].as_str().unwrap().to_owned();
    let face = format!("{pad}:side({})", segment(&region, &left));
    let circle_sketch = command(&mut d, json!({"cmd": "sketch.create"}));
    let circle_sketch = circle_sketch["uid"].as_str().unwrap().to_owned();
    let circle = command(
        &mut d,
        json!({"cmd": "sketch.add_circle", "sketch": circle_sketch, "center": [120, 20],
               "diameter": 6}),
    );
    let circle_region = circle["region"].as_str().unwrap().to_owned();
    let next = command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": circle_sketch, "region": circle_region}],
               "extent": {"type": "to_object", "object": {"type": "face",
                          "body": format!("{pad}.b0"), "face": face}},
               "operation": "new_body"}}),
    );
    let next = next["uid"].as_str().unwrap().to_owned();
    assert_eq!(status(&d, &pad), FeatureStatus::Ok);
    assert_eq!(status(&d, &next), FeatureStatus::Ok);
    move_source(&mut d, GAP);
    let s = sketch(&d, "F3");
    assert_eq!(s["error"], Value::Null, "{s}");
    assert_eq!(rectangle_region(&s), region);
    let at = line_x(&s, &left);
    assert!(close(at[0], 61.0) && close(at[1], 61.0), "{at:?}");
    assert_eq!(status(&d, &pad), FeatureStatus::Ok, "{pad}");
    assert_eq!(status(&d, &next), FeatureStatus::Ok, "{next}");
}
