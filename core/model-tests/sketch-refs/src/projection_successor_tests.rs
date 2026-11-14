// SPDX-License-Identifier: MIT
//! A linked projection whose source edge was merged with, or split from,
//! edges on the same curve (mitcad#1784), with the mock kernel: an arc of a
//! rim that became one whole circle follows it (a lost source, its
//! successor found between the same faces), with the hole on its centre,
//! also when the disc moves, and becomes that circle when nothing uses its
//! ends (mitcad#1794), else keeps its span; a whole circle split into arcs
//! follows the circle through them; a line split in two follows the piece
//! that holds its used end; two edges on two lines leave a lost source as
//! it was, listed for Project Again, which takes the one named. 3D
//! sketches (mitcad#1794): a reference to an edge that is gone follows the
//! one edge between the same faces, and a converted sketch's kept
//! projection follows as a planar one.

use std::f64::consts::{PI, TAU};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::kernel::Curve3;
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
    let query = json!({"query": "sketch", "uid": "F3"});
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn near(p: &Value, q: [f64; 2]) -> bool {
    close(p[0].as_f64().unwrap(), q[0]) && close(p[1].as_f64().unwrap(), q[1])
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

/// Where a point of the sketch is.
fn at(s: &Value, id: &str) -> Value {
    entity(s, id)["at"].clone()
}

/// The top face of the block.
const FACE: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";
/// A side face of the block.
const SIDE: &str = "F2:side(c1[c4,c2])";

/// The edge between the top face and the side face: the rim as it is when
/// it is one edge.
fn rim() -> String {
    format!("E{{{FACE}|{SIDE}}}")
}

/// The edge between a piece of the top face and the side face: an arc of
/// the rim as it was when the corners of a prism on the top face split
/// it, a name the shape no longer has.
fn rim_piece() -> String {
    format!("E{{{FACE}#3|{SIDE}}}")
}

/// A 60 x 40 block 20 high (F1, F2), the mock kernel's edge curves set by
/// each test.
fn block() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    command(
        &mut d,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    d
}

/// The model changes (the block gets wider), its edges' curves as the
/// kernel gives them now.
fn rebuild(d: &mut Document<MockKernel>, width: f64) {
    let changed = command(
        d,
        json!({"cmd": "set_parameter", "name": "d1", "value": width}),
    );
    assert_eq!(changed["error"], Value::Null, "{changed}");
}

/// An arc or circle on the top face (z = 20), counter-clockwise from
/// `start` to `end` (radians).
fn round(center: [f64; 2], radius: f64, start: f64, end: f64) -> Curve3 {
    Curve3::Conic {
        center: [center[0], center[1], 20.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: radius,
        minor: radius,
        start,
        end,
        closed: end - start >= TAU,
    }
}

fn line(a: [f64; 2], b: [f64; 2]) -> Curve3 {
    Curve3::Line {
        start: [a[0], a[1], 20.0],
        end: [b[0], b[1], 20.0],
    }
}

fn fixed(e: Value) -> Value {
    let mut e = e;
    e["fixed"] = json!(true);
    e["reference"] = json!(true);
    e
}

/// The sketch on the top face (F3) with the linked projection of `source`
/// as `projected` (its entities, all fixed references), and the user's
/// `own` entities.
fn with_projection(d: &mut Document<MockKernel>, source: &str, projected: &[Value], own: &[Value]) {
    let ids: Vec<Value> = projected.iter().map(|e| e["id"].clone()).collect();
    let mut entities: Vec<Value> = projected.iter().cloned().map(fixed).collect();
    entities.extend(own.iter().cloned());
    command(
        d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": FACE},
               "entities": entities,
               "projections": [{"source": source, "body": "F2.b0", "by_geometry": true,
                                "entities": ids}]}}),
    );
}

fn infos(d: &Document<MockKernel>) -> Vec<String> {
    d.infos("F3".parse().unwrap()).to_vec()
}

/// The projected arc of the rim (centre p1, ends p2 and p3, curve c4), a
/// piece of it as it was when the corners of a prism split it; the rim is
/// one circle of radius 32 about (30, 20) now, under another name.
fn arc_of_one_circle(d: &mut Document<MockKernel>, own: &[Value]) {
    d.kernel()
        .named_curves
        .borrow_mut()
        .insert(rim(), round([30.0, 20.0], 32.0, 0.0, TAU));
    with_projection(
        d,
        &rim_piece(),
        &[
            json!({"id": "p1", "type": "point", "at": [30.0, 20.0]}),
            json!({"id": "p2", "type": "point", "at": [61.5, 20.0]}),
            json!({"id": "p3", "type": "point", "at": [30.0, 51.5]}),
            json!({"id": "c4", "type": "arc", "center": "p1", "start": "p2", "end": "p3"}),
        ],
        own,
    );
}

/// Whether the sketch query lists an entity.
fn has(s: &Value, id: &str) -> bool {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["id"] == id)
}

/// (a) The projected arc of a rim the corners of a prism had split, the
/// corners moved off it: the rim is one circle, of a larger radius, under
/// another name. Nothing uses the arc's ends (the hole only its centre), so
/// it becomes that whole circle (mitcad#1794), its ends gone, its centre
/// where the hole is, also when the disc moves; an information notice, no
/// warning. An edit keeps the circle, which follows from then on.
#[test]
fn an_arc_of_a_rim_that_became_one_circle_follows_it() {
    let mut d = block();
    // The hole on the arc's centre.
    arc_of_one_circle(
        &mut d,
        &[json!({"id": "c5", "type": "circle", "center": "p1", "radius": 16.0})],
    );
    let s = sketch(&d);
    assert_eq!(s["warnings"], json!([]), "{s}");
    let projection = &s["projections"][0];
    assert_eq!(projection["successor"], json!(rim()), "{s}");
    assert!(projection.get("lost").is_none(), "{s}");
    // The arc is the whole circle now, its ends gone; the hole stays on its
    // centre.
    let arc = entity(&s, "c4");
    assert_eq!(arc["type"], "circle", "{s}");
    assert!(
        close(arc["geometry"]["radius"].as_f64().unwrap(), 32.0),
        "{s}"
    );
    assert!(near(&arc["geometry"]["center"], [30.0, 20.0]), "{s}");
    assert!(!has(&s, "p2") && !has(&s, "p3"), "{s}");
    assert_eq!(projection["entities"], json!(["p1", "c4"]), "{s}");
    assert!(near(&at(&s, "p1"), [30.0, 20.0]), "{s}");
    assert!(near(&entity(&s, "c5")["geometry"]["center"], [30.0, 20.0]));
    let infos = infos(&d);
    assert_eq!(infos.len(), 1, "{infos:?}");
    assert!(
        infos[0].contains(&rim_piece()) && infos[0].contains(&rim()),
        "{infos:?}"
    );
    assert!(infos[0].contains("merged or split"), "{infos:?}");

    // The disc moves: the arc's centre, and the hole, with it.
    d.kernel()
        .named_curves
        .borrow_mut()
        .insert(rim(), round([40.0, 20.0], 32.0, 0.0, TAU));
    rebuild(&mut d, 70.0);
    let s = sketch(&d);
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert!(near(&at(&s, "p1"), [40.0, 20.0]), "{s}");
    assert_eq!(entity(&s, "c4")["type"], "circle", "{s}");
    assert!(near(&entity(&s, "c5")["geometry"]["center"], [40.0, 20.0]));

    // An edit starts from the sketch as it evaluated: the circle is kept,
    // its ends gone from the stored sketch too, and it follows the rim
    // from then on.
    command(
        &mut d,
        json!({"cmd": "sketch.add_point", "sketch": "F3", "at": [0.0, 0.0]}),
    );
    let query = json!({"query": "feature", "uid": "F3"});
    let feature: Value = serde_json::from_str(&d.query(&query.to_string()).unwrap()).unwrap();
    let stored = &feature["def"];
    let ids: Vec<&Value> = stored["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| &e["id"])
        .collect();
    assert!(
        !ids.contains(&&json!("p2")) && !ids.contains(&&json!("p3")),
        "{stored}"
    );
    let s = sketch(&d);
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert_eq!(entity(&s, "c4")["type"], "circle", "{s}");
    assert!(
        near(&entity(&s, "c4")["geometry"]["center"], [40.0, 20.0]),
        "{s}"
    );
}

/// (a2) The same arc with an end a line of the user's starts on: it keeps
/// its span on the circle (mitcad#1784), its kind with it.
#[test]
fn an_arc_with_a_used_end_keeps_its_span_on_the_whole_circle() {
    let mut d = block();
    arc_of_one_circle(
        &mut d,
        &[
            json!({"id": "p5", "type": "point", "at": [80.0, 20.0]}),
            json!({"id": "c6", "type": "line", "start": "p2", "end": "p5"}),
        ],
    );
    let s = sketch(&d);
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert_eq!(s["projections"][0]["successor"], json!(rim()), "{s}");
    assert_eq!(entity(&s, "c4")["type"], "arc", "{s}");
    assert!(near(&at(&s, "p2"), [62.0, 20.0]), "{s}");
    assert!(near(&at(&s, "p3"), [30.0, 52.0]), "{s}");
}

/// (a3) The same arc whose length a dimension measures: a circle has no
/// arc length, so it keeps its span (mitcad#1794).
#[test]
fn an_arc_whose_length_is_measured_keeps_its_span() {
    let mut d = block();
    d.kernel()
        .named_curves
        .borrow_mut()
        .insert(rim(), round([30.0, 20.0], 32.0, 0.0, TAU));
    let entities: Vec<Value> = [
        json!({"id": "p1", "type": "point", "at": [30.0, 20.0]}),
        json!({"id": "p2", "type": "point", "at": [61.5, 20.0]}),
        json!({"id": "p3", "type": "point", "at": [30.0, 51.5]}),
        json!({"id": "c4", "type": "arc", "center": "p1", "start": "p2", "end": "p3"}),
    ]
    .into_iter()
    .map(fixed)
    .collect();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": FACE},
               "entities": entities,
               "dimensions": [{"id": "k5", "type": "arc_length", "arc": "c4", "driven": true}],
               "projections": [{"source": rim_piece(), "body": "F2.b0", "by_geometry": true,
                                "entities": ["p1", "p2", "p3", "c4"]}]}}),
    );
    let s = sketch(&d);
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert_eq!(entity(&s, "c4")["type"], "arc", "{s}");
    assert!(near(&at(&s, "p2"), [62.0, 20.0]), "{s}");
}

/// (b) The reverse: the projected whole rim split into arcs, its name
/// finding them now. The circle follows the circle through them, its
/// centre with the hole on it; and the circle again when it is whole.
#[test]
fn a_circle_split_into_arcs_follows_the_circle_through_them() {
    let mut d = block();
    *d.kernel().curves.borrow_mut() = vec![round([30.0, 20.0], 31.5, 0.0, TAU)];
    with_projection(
        &mut d,
        &rim(),
        &[
            json!({"id": "p1", "type": "point", "at": [30.0, 20.0]}),
            json!({"id": "c2", "type": "circle", "center": "p1", "radius": 31.5}),
        ],
        &[json!({"id": "c3", "type": "circle", "center": "p1", "radius": 16.0})],
    );
    assert_eq!(sketch(&d)["warnings"], json!([]));
    assert!(infos(&d).is_empty());
    // Split into two arcs of a larger radius.
    *d.kernel().curves.borrow_mut() = vec![
        round([30.0, 20.0], 32.0, 0.0, PI),
        round([30.0, 20.0], 32.0, PI, TAU),
    ];
    rebuild(&mut d, 70.0);
    let s = sketch(&d);
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert_eq!(s["projections"][0]["successor"], json!(rim()), "{s}");
    let circle = entity(&s, "c2");
    assert!(
        close(circle["geometry"]["radius"].as_f64().unwrap(), 32.0),
        "{s}"
    );
    assert!(near(&at(&s, "p1"), [30.0, 20.0]), "{s}");
    assert!(infos(&d)[0].contains("merged or split"), "{:?}", infos(&d));
    // Whole again, moved: it follows as any circle does.
    *d.kernel().curves.borrow_mut() = vec![round([35.0, 20.0], 32.0, 0.0, TAU)];
    rebuild(&mut d, 80.0);
    let s = sketch(&d);
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert!(s["projections"][0].get("successor").is_none(), "{s}");
    assert!(near(&at(&s, "p1"), [35.0, 20.0]), "{s}");
    assert!(infos(&d).is_empty());
}

/// (c) A projected line split in two by a new face: it follows the piece
/// that holds its used end (the end a line of the user's starts on), not
/// the piece nearest to it as a whole.
#[test]
fn a_split_line_follows_the_piece_that_holds_its_used_end() {
    let mut d = block();
    *d.kernel().curves.borrow_mut() = vec![line([0.0, 0.0], [60.0, 0.0])];
    with_projection(
        &mut d,
        &rim(),
        &[
            json!({"id": "p1", "type": "point", "at": [0.0, 0.0]}),
            json!({"id": "p2", "type": "point", "at": [60.0, 0.0]}),
            json!({"id": "c3", "type": "line", "start": "p1", "end": "p2"}),
        ],
        &[
            json!({"id": "p4", "type": "point", "at": [10.0, 30.0]}),
            json!({"id": "c5", "type": "line", "start": "p1", "end": "p4"}),
        ],
    );
    assert_eq!(sketch(&d)["warnings"], json!([]));
    // A slot across it: two pieces, the longer one away from the used end.
    *d.kernel().curves.borrow_mut() = vec![
        line([0.0, 0.0], [20.0, 0.0]),
        line([30.0, 0.0], [60.0, 0.0]),
    ];
    rebuild(&mut d, 70.0);
    let s = sketch(&d);
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert_eq!(s["projections"][0]["successor"], json!(rim()), "{s}");
    assert!(near(&at(&s, "p1"), [0.0, 0.0]), "{s}");
    assert!(near(&at(&s, "p2"), [20.0, 0.0]), "{s}");
    assert!(infos(&d)[0].contains("merged or split"), "{:?}", infos(&d));
}

/// (d) Two edges between the same faces on two lines: none alone took the
/// lost source's place. It keeps its geometry and warns as before, lists
/// them, and Project Again takes the one named as its source.
#[test]
fn two_candidates_warn_and_project_again_takes_the_one_named() {
    let mut d = block();
    let first = format!("{}#0", rim());
    let second = format!("{}#1", rim());
    *d.kernel().extra_edges.borrow_mut() = vec![first.clone(), second.clone()];
    {
        let mut named = d.kernel().named_curves.borrow_mut();
        named.insert(first.clone(), line([0.0, 0.0], [60.0, 0.0]));
        named.insert(second.clone(), line([0.0, 5.0], [60.0, 5.0]));
    }
    with_projection(
        &mut d,
        &rim_piece(),
        &[
            json!({"id": "p1", "type": "point", "at": [0.0, 0.0]}),
            json!({"id": "p2", "type": "point", "at": [50.0, 0.0]}),
            json!({"id": "c3", "type": "line", "start": "p1", "end": "p2"}),
        ],
        &[],
    );
    let s = sketch(&d);
    let projection = &s["projections"][0];
    assert_eq!(projection["lost"], true, "{s}");
    assert_eq!(projection["candidates"], json!([&first, &second]), "{s}");
    // Each described for a person's list (mitcad#1794): two lines of 60.
    for described in projection["candidate_curves"].as_array().unwrap() {
        assert_eq!(described["type"], "line", "{s}");
        assert!(close(described["length"].as_f64().unwrap(), 60.0), "{s}");
    }
    assert_eq!(projection["candidate_curves"].as_array().unwrap().len(), 2);
    assert!(projection.get("successor").is_none(), "{s}");
    let warnings = s["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{s}");
    assert!(warnings[0].as_str().unwrap().contains("it is gone"), "{s}");
    assert!(near(&at(&s, "p2"), [50.0, 0.0]), "{s}");
    assert!(infos(&d).is_empty());
    // Project Again asks which.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.project_again", "sketch": "F3", "projection": 0}),
    );
    assert!(error.contains("successor"), "{error}");
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.project_again", "sketch": "F3", "projection": 0,
               "successor": rim()}),
    );
    assert!(error.contains("none of those"), "{error}");
    // The first named: it becomes the source, projected again from it.
    command(
        &mut d,
        json!({"cmd": "sketch.project_again", "sketch": "F3", "successor": &first}),
    );
    let s = sketch(&d);
    assert_eq!(s["projections"][0]["source"], json!(&first), "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");
    let lines: Vec<&Value> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "line")
        .collect();
    assert_eq!(lines.len(), 1, "{s}");
    let g = &lines[0]["geometry"];
    assert!(
        (near(&g["start"], [0.0, 0.0]) && near(&g["end"], [60.0, 0.0]))
            || (near(&g["start"], [60.0, 0.0]) && near(&g["end"], [0.0, 0.0])),
        "{s}"
    );
}

/// The `sketch3d` query of F3.
fn sketch3d(d: &Document<MockKernel>) -> Value {
    let query = json!({"query": "sketch3d", "uid": "F3"});
    serde_json::from_str(&d.query(&query.to_string()).unwrap()).unwrap()
}

/// (mitcad#1794, 4) A 3D sketch's point at the middle of an edge that is
/// gone (a piece of the top face's edge as a prism's corners had split it):
/// the one edge between the same faces took its place, and the point is at
/// its middle, with an information notice that names both for agents.
/// With a second edge between those faces on another line, none alone
/// did: the record fails as before, the agents' text listing both.
#[test]
fn a_3d_reference_to_a_gone_edge_follows_the_one_that_took_its_place() {
    let mut d = block();
    d.kernel()
        .named_curves
        .borrow_mut()
        .insert(rim(), line([0.0, 0.0], [60.0, 0.0]));
    command(&mut d, json!({"cmd": "sketch3d.create"}));
    let made = command(
        &mut d,
        json!({"cmd": "sketch3d.add_point", "sketch": "F3", "at": [29, 1, 19],
               "source": {"body": "F2.b0", "edge": rim_piece(), "at": "middle"}}),
    );
    let point = made["made"][0].clone();
    let s = sketch3d(&d);
    assert!(s.get("failure").is_none_or(Value::is_null), "{s}");
    let at = s["points"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == point)
        .unwrap_or_else(|| panic!("no {point} in {s}"))["at"]
        .clone();
    let at: Vec<f64> = at
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert!(
        close(at[0], 30.0) && close(at[1], 0.0) && close(at[2], 20.0),
        "{s}"
    );
    let infos = infos(&d);
    assert!(
        infos
            .iter()
            .any(|i| i.contains("merged or split") && i.contains(&rim())),
        "{infos:?}"
    );

    // Another edge between the same faces, on another line.
    let other = format!("{}#1", rim());
    d.kernel().extra_edges.borrow_mut().push(other.clone());
    d.kernel()
        .named_curves
        .borrow_mut()
        .insert(other.clone(), line([0.0, 5.0], [60.0, 5.0]));
    // The change fails the 3D sketch, as it should: no rebuild() here,
    // which wants no error.
    let changed = command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 70.0}),
    );
    assert!(
        changed["error"]
            .as_str()
            .is_some_and(|e| e.contains("does not exist")),
        "{changed}"
    );
    let s = sketch3d(&d);
    let failure = &s["failure"];
    assert_eq!(failure["kind"], "reference", "{s}");
    let detail = failure["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("could have taken its place") && detail.contains(&other),
        "{s}"
    );
}

/// (mitcad#1794, 4) A planar sketch's linked projection of an arc of the
/// rim kept by its conversion into a 3D sketch (mitcad#1550) follows the
/// rim that became one whole circle as the planar sketch follows it: no
/// warning, the information notice instead.
#[test]
fn a_converted_sketchs_kept_projection_follows_the_edge_that_took_its_place() {
    let mut d = block();
    arc_of_one_circle(
        &mut d,
        &[json!({"id": "c5", "type": "circle", "center": "p1", "radius": 16.0})],
    );
    let converted = command(&mut d, json!({"cmd": "sketch3d.convert", "sketch": "F3"}));
    assert_eq!(converted["type"], "spatial_sketch", "{converted}");
    let s = sketch3d(&d);
    assert!(s.get("failure").is_none_or(Value::is_null), "{s}");
    let warnings = d.warnings("F3".parse().unwrap()).to_vec();
    assert!(
        !warnings.iter().any(|w| w.contains("does not follow")),
        "{warnings:?}"
    );
    let infos = infos(&d);
    assert!(
        infos.iter().any(|i| i.contains("merged or split")),
        "{infos:?}"
    );
}
