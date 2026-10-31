// SPDX-License-Identifier: MIT
//! Linked projections that follow their source by geometry, warn when they
//! cannot, and Clean Up Projections (mitcad#232), with the mock kernel.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::TopoName;
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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "sketch", "uid": uid}))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn areas(doc: &Document<MockKernel>, uid: &str) -> Vec<f64> {
    let mut areas: Vec<f64> = sketch(doc, uid)["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["area"].as_f64().unwrap())
        .collect();
    areas.sort_by(f64::total_cmp);
    areas
}

/// The sketch F3 has one region, of this area.
fn assert_area(doc: &Document<MockKernel>, area: f64) {
    let found = areas(doc, "F3");
    assert!(found.len() == 1 && close(found[0], area), "{found:?}");
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

fn has(s: &Value, id: &str) -> bool {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["id"] == id)
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    let e = entity(s, id);
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

fn at_close(s: &Value, id: &str, expected: [f64; 2]) {
    let p = at(s, id);
    assert!(
        close(p[0], expected[0]) && close(p[1], expected[1]),
        "{id} at {p:?}, not {expected:?}"
    );
}

/// The top face of the block.
const FACE: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";

/// The edge of the top face over the block's side on c1, as the kernel
/// names it.
fn bottom_edge() -> String {
    format!("E{{{FACE}|F2:side(c1[c4,c2])}}")
        .parse::<TopoName>()
        .unwrap()
        .to_string()
}

/// A 60 x 40 block 20 high (F1, F2).
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

fn line(a: [f64; 2], b: [f64; 2]) -> Curve3 {
    Curve3::Line {
        start: [a[0], a[1], 20.0],
        end: [b[0], b[1], 20.0],
    }
}

/// The top face's edges for a block `width` wide, from the corner
/// `first` (0 to 3) on, counter-clockwise.
fn square(width: f64, first: usize) -> Vec<Curve3> {
    let c = [[0.0, 0.0], [width, 0.0], [width, 40.0], [0.0, 40.0]];
    (0..4)
        .map(|i| line(c[(first + i) % 4], c[(first + i + 1) % 4]))
        .collect()
}

/// The block's width changes (as a sketch dimension of it would) and the
/// kernel's face edges with it.
fn widen(d: &mut Document<MockKernel>, curves: Vec<Curve3>) {
    *d.kernel().curves.borrow_mut() = curves;
    let changed = command(
        d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 80}),
    );
    assert_eq!(changed["error"], Value::Null, "{changed}");
}

fn feature(d: &Document<MockKernel>, uid: &str) -> Value {
    query(d, json!({"query": "feature", "uid": uid}))
}

/// The id of the projected line from `a` to `b` (either way round).
fn line_id(s: &Value, a: [f64; 2], b: [f64; 2]) -> String {
    let near = |p: &Value, q: [f64; 2]| {
        close(p[0].as_f64().unwrap(), q[0]) && close(p[1].as_f64().unwrap(), q[1])
    };
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            let g = &e["geometry"];
            e["type"] == "line"
                && ((near(&g["start"], a) && near(&g["end"], b))
                    || (near(&g["start"], b) && near(&g["end"], a)))
        })
        .unwrap_or_else(|| panic!("no line {a:?}-{b:?} in {s}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_projection_of_some_of_a_faces_edges_follows() {
    // As an import records it: the face as the source, two of its edges.
    let mut d = block();
    *d.kernel().curves.borrow_mut() = square(60.0, 0);
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": FACE},
               "entities": [
                   {"id": "p1", "type": "point", "at": [0.0, 0.0], "fixed": true, "reference": true},
                   {"id": "p2", "type": "point", "at": [60.0, 0.0], "fixed": true, "reference": true},
                   {"id": "p3", "type": "point", "at": [60.0, 40.0], "fixed": true, "reference": true},
                   {"id": "c4", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
                   {"id": "c5", "type": "line", "start": "p2", "end": "p3", "fixed": true, "reference": true}],
               "projections": [{"source": FACE, "body": "F2.b0", "by_geometry": true,
                                "entities": ["p1", "p2", "p3", "c4", "c5"]}]}}),
    );
    widen(&mut d, square(80.0, 0));
    let s = sketch(&d, "F3");
    at_close(&s, "p1", [0.0, 0.0]);
    at_close(&s, "p2", [80.0, 0.0]);
    at_close(&s, "p3", [80.0, 40.0]);
    // The face's two other edges follow nothing: no warning, but the
    // query says so.
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert_eq!(s["projections"][0]["unmatched"], 2, "{s}");
    assert_eq!(feature(&d, "F3")["status"], "ok");
}

#[test]
fn a_face_whose_edges_come_back_in_another_order_follows() {
    let mut d = block();
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}}),
    );
    *d.kernel().curves.borrow_mut() = square(60.0, 0);
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    let s = sketch(&d, "F3");
    let bottom = line_id(&s, [0.0, 0.0], [60.0, 0.0]);
    let right = line_id(&s, [60.0, 0.0], [60.0, 40.0]);
    // A dimension on the bottom edge measures it, wherever the kernel
    // lists it.
    let driven = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F3",
               "dimension": {"type": "length", "line": bottom}, "driven": true}),
    );
    let k = driven["dimensions"][0].as_str().unwrap().to_owned();
    // The kernel now starts from another corner (the same kinds in the
    // same pattern: by position each line would take its neighbour's
    // place).
    widen(&mut d, square(80.0, 2));
    let s = sketch(&d, "F3");
    assert_eq!(line_id(&s, [0.0, 0.0], [80.0, 0.0]), bottom, "{s}");
    assert_eq!(line_id(&s, [80.0, 0.0], [80.0, 40.0]), right, "{s}");
    let measured = s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == k.as_str())
        .unwrap()["measured"]
        .as_f64()
        .unwrap();
    assert!(close(measured, 80.0), "{measured}");
    assert_area(&d, 3200.0);
    assert_eq!(s["warnings"], json!([]));
}

#[test]
fn a_face_that_gains_an_edge_keeps_its_entities_and_reports_the_new_one() {
    let mut d = block();
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}}),
    );
    *d.kernel().curves.borrow_mut() = square(60.0, 0);
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    let s = sketch(&d, "F3");
    let top = line_id(&s, [60.0, 40.0], [0.0, 40.0]);
    let left = line_id(&s, [0.0, 40.0], [0.0, 0.0]);
    let count = s["entities"].as_array().unwrap().len();
    // A chamfer on the corner at (80, 40) adds an edge.
    widen(
        &mut d,
        vec![
            line([0.0, 0.0], [80.0, 0.0]),
            line([80.0, 0.0], [80.0, 38.0]),
            line([80.0, 38.0], [78.0, 40.0]),
            line([78.0, 40.0], [0.0, 40.0]),
            line([0.0, 40.0], [0.0, 0.0]),
        ],
    );
    let s = sketch(&d, "F3");
    assert_eq!(s["entities"].as_array().unwrap().len(), count);
    line_id(&s, [0.0, 0.0], [80.0, 0.0]);
    assert_eq!(line_id(&s, [0.0, 40.0], [0.0, 0.0]), left);
    // The top line follows the top edge (its far end where the right
    // edge ends too: the old corner is one point).
    let t = entity(&s, &top);
    let start = t["geometry"]["start"][0].as_f64().unwrap();
    assert!(close(start, 78.0) || close(start, 80.0), "{t}");
    assert_eq!(s["projections"][0]["unmatched"], 1, "{s}");
    assert!(s["projections"][0].get("unfollowed").is_none(), "{s}");
    assert_eq!(s["warnings"], json!([]));
}

#[test]
fn a_projection_that_cannot_follow_warns_on_the_sketch() {
    let mut d = block();
    let circle = Curve3::Conic {
        center: [30.0, 20.0, 20.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: 5.0,
        minor: 5.0,
        start: 0.0,
        end: std::f64::consts::TAU,
        closed: true,
    };
    *d.kernel().curves.borrow_mut() = vec![circle];
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": FACE},
               "entities": [
                   {"id": "p1", "type": "point", "at": [30.0, 20.0], "fixed": true, "reference": true},
                   {"id": "c2", "type": "circle", "center": "p1", "radius": 5.0,
                    "fixed": true, "reference": true}],
               "projections": [{"source": FACE, "body": "F2.b0", "by_geometry": true, "entities": ["p1", "c2"]}]}}),
    );
    assert_eq!(feature(&d, "F3")["status"], "ok");
    // The source no longer has a circle: the circle keeps its geometry and
    // the sketch says so, on the timeline and in the sketch query.
    widen(&mut d, square(80.0, 0));
    let s = sketch(&d, "F3");
    assert_eq!(entity(&s, "c2")["radius"], 5.0);
    assert_eq!(s["projections"][0]["unfollowed"], json!(["c2"]), "{s}");
    let warnings = s["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{s}");
    let warning = warnings[0].as_str().unwrap();
    assert!(
        warning.contains("does not follow") && warning.contains("c2"),
        "{warning}"
    );
    let f = feature(&d, "F3");
    assert_eq!(f["status"], "warning", "{f}");
    // A source that is gone too.
    d.kernel().curves.borrow_mut().clear();
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 90}),
    );
    let s = sketch(&d, "F3");
    assert_eq!(s["projections"][0]["lost"], true, "{s}");
    assert!(
        s["warnings"][0].as_str().unwrap().contains("it is gone"),
        "{s}"
    );
}

#[test]
fn a_curve_a_source_lists_twice_is_projected_once() {
    let mut d = block();
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}}),
    );
    // A seam edge listed twice, the other way round the second time.
    let mut curves = square(60.0, 0);
    curves.push(line([60.0, 0.0], [0.0, 0.0]));
    *d.kernel().curves.borrow_mut() = curves;
    let projected = command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    assert_eq!(
        projected["entities"].as_array().unwrap().len(),
        8,
        "{projected}"
    );
    // Following, the repeat is no curve nothing follows.
    let mut curves = square(80.0, 0);
    curves.push(line([80.0, 0.0], [0.0, 0.0]));
    widen(&mut d, curves);
    let s = sketch(&d, "F3");
    assert!(s["projections"][0].get("unmatched").is_none(), "{s}");
    assert_area(&d, 3200.0);
}

/// A sketch on the top face whose projection of the face holds the
/// bottom edge twice, each with its own points (as an import may), a
/// line of the user's from the repeat's end, perpendicular to it, and a
/// driven dimension on the repeat.
fn with_a_repeat() -> Document<MockKernel> {
    let mut d = block();
    *d.kernel().curves.borrow_mut() = square(60.0, 0);
    let fixed = |e: Value| {
        let mut e = e;
        e["fixed"] = json!(true);
        e["reference"] = json!(true);
        e
    };
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": FACE},
               "entities": [
                   fixed(json!({"id": "p1", "type": "point", "at": [0.0, 0.0]})),
                   fixed(json!({"id": "p2", "type": "point", "at": [60.0, 0.0]})),
                   fixed(json!({"id": "p3", "type": "point", "at": [60.0, 40.0]})),
                   fixed(json!({"id": "p4", "type": "point", "at": [0.0, 40.0]})),
                   fixed(json!({"id": "c5", "type": "line", "start": "p1", "end": "p2"})),
                   fixed(json!({"id": "c6", "type": "line", "start": "p2", "end": "p3"})),
                   fixed(json!({"id": "c7", "type": "line", "start": "p3", "end": "p4"})),
                   fixed(json!({"id": "c8", "type": "line", "start": "p4", "end": "p1"})),
                   fixed(json!({"id": "p9", "type": "point", "at": [60.0, 0.0]})),
                   fixed(json!({"id": "p10", "type": "point", "at": [0.0, 0.0]})),
                   fixed(json!({"id": "c11", "type": "line", "start": "p9", "end": "p10"})),
                   {"id": "p12", "type": "point", "at": [60.0, -15.0]},
                   {"id": "c13", "type": "line", "start": "p9", "end": "p12"}],
               "constraints": [{"id": "k14", "type": "perpendicular", "a": "c13", "b": "c11"}],
               "dimensions": [{"id": "k15", "type": "length", "line": "c11", "driven": true}],
               "projections": [{"source": FACE, "body": "F2.b0", "by_geometry": true,
                                "entities": ["p1", "p2", "p3", "p4", "c5", "c6", "c7", "c8",
                                             "p9", "p10", "c11"]}]}}),
    );
    d
}

#[test]
fn a_repeated_curve_follows_with_its_twin_and_the_clean_up_removes_it() {
    let mut d = with_a_repeat();
    widen(&mut d, square(80.0, 0));
    let s = sketch(&d, "F3");
    // Both follow the one bottom edge; nothing is unfollowed.
    assert_eq!(s["warnings"], json!([]), "{s}");
    at_close(&s, "p9", [80.0, 0.0]);
    assert!(close(at(&s, "p12")[0], 80.0), "{s}");
    // Clean up: c11 goes into c5, its points into c5's, and what was on
    // them is on c5 now.
    let cleaned = command(
        &mut d,
        json!({"cmd": "sketch.clean_up_projections", "sketch": "F3"}),
    );
    assert_eq!(
        cleaned["cleaned"],
        json!([{"curve": "c11", "kept": "c5", "how": "same", "link": null}]),
        "{cleaned}"
    );
    let s = sketch(&d, "F3");
    assert!(!has(&s, "c11") && !has(&s, "p9") && !has(&s, "p10"), "{s}");
    assert_eq!(entity(&s, "c13")["start"], "p2", "{s}");
    let constraint = &s["constraints"][0];
    assert_eq!(
        (&constraint["a"], &constraint["b"]),
        (&json!("c13"), &json!("c5"))
    );
    let dimension = &s["dimensions"][0];
    assert_eq!(dimension["line"], "c5", "{s}");
    assert!(close(dimension["measured"].as_f64().unwrap(), 80.0), "{s}");
    assert_eq!(
        s["projections"][0]["entities"],
        json!(["p1", "p2", "p3", "p4", "c5", "c6", "c7", "c8"])
    );
    // One undo step; and nothing left to clean up.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.clean_up_projections", "sketch": "F3"}),
    );
    assert!(error.contains("nothing to clean up"), "{error}");
    command(&mut d, json!({"cmd": "undo"}));
    assert!(has(&sketch(&d, "F3"), "c11"));
}

/// A sketch on the top face with the bottom edge projected (linked) and
/// the face projected after it (linked), and a shorter line of the
/// user's made reference on the bottom (`sketch.set_fixed` keeps it a
/// curve of its own).
fn with_two_sources() -> Document<MockKernel> {
    let mut d = block();
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}}),
    );
    let edge = bottom_edge();
    d.kernel()
        .named_curves
        .borrow_mut()
        .insert(edge.clone(), line([10.0, 0.0], [50.0, 0.0]));
    *d.kernel().curves.borrow_mut() = square(60.0, 0);
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": edge, "body": "F2.b0", "linked": true}),
    );
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    d
}

#[test]
fn overlapping_projections_of_two_sources_make_the_regions_of_one() {
    let mut d = with_two_sources();
    let s = sketch(&d, "F3");
    assert_eq!(s["projections"].as_array().unwrap().len(), 2);
    assert_area(&d, 2400.0);
    // Nothing of either is removed by following or evaluating.
    widen(&mut d, square(80.0, 0));
    let s = sketch(&d, "F3");
    assert_eq!(s["projections"].as_array().unwrap().len(), 2);
    assert_area(&d, 3200.0);
}

#[test]
fn the_clean_up_merges_a_line_on_another_and_says_what_changes() {
    let mut d = with_two_sources();
    let s = sketch(&d, "F3");
    let short = line_id(&s, [10.0, 0.0], [50.0, 0.0]);
    let bottom = line_id(&s, [0.0, 0.0], [60.0, 0.0]);
    // An extrude of the sketch's region, which the short line (the lower
    // id) names.
    let key = s["regions"][0]["key"].as_str().unwrap().to_owned();
    assert!(key.contains(&format!("{short}[")), "{key}");
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": key}],
               "extent": {"type": "distance", "distance": 5}, "operation": "join"}}),
    );
    // Without merging there is nothing the same.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.clean_up_projections", "sketch": "F3", "merge": false}),
    );
    assert!(error.contains("nothing to clean up"), "{error}");
    let cleaned = command(
        &mut d,
        json!({"cmd": "sketch.clean_up_projections", "sketch": "F3"}),
    );
    assert_eq!(
        cleaned["cleaned"][0],
        json!({"curve": short, "kept": bottom, "how": "on", "link": bottom_edge()}),
        "{cleaned}"
    );
    let warnings: Vec<&str> = cleaned["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    assert!(
        warnings.iter().any(|w| w.contains("on a longer line")),
        "{warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("followed") && w.contains("not split again")),
        "{warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("Extrude2 uses the region") && w.contains(&key)),
        "{warnings:?}"
    );
    let s = sketch(&d, "F3");
    assert!(!has(&s, &short));
    assert_eq!(s["projections"].as_array().unwrap().len(), 1, "{s}");
    assert_area(&d, 2400.0);
}

#[test]
fn the_clean_up_keeps_the_side_of_an_angle_on_a_line_that_ran_the_other_way() {
    // As `with_two_sources`, the short edge projected from right to left.
    let mut d = block();
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}}),
    );
    let edge = bottom_edge();
    d.kernel()
        .named_curves
        .borrow_mut()
        .insert(edge.clone(), line([50.0, 0.0], [10.0, 0.0]));
    *d.kernel().curves.borrow_mut() = square(60.0, 0);
    for source in [edge.as_str(), FACE] {
        command(
            &mut d,
            json!({"cmd": "sketch.project", "sketch": "F3", "source": source, "body": "F2.b0", "linked": true}),
        );
    }
    let s = sketch(&d, "F3");
    let short = line_id(&s, [10.0, 0.0], [50.0, 0.0]);
    let bottom = line_id(&s, [0.0, 0.0], [60.0, 0.0]);
    let run = |id: &str| {
        let g = &entity(&s, id)["geometry"];
        g["end"][0].as_f64().unwrap() - g["start"][0].as_f64().unwrap()
    };
    assert!(run(&short) * run(&bottom) < 0.0, "{s}");
    let slanted = command(
        &mut d,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [20, 10], "end": [30, 40]}),
    );
    let slanted = slanted["entities"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    let added = command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F3", "driven": true,
               "dimension": {"type": "angle", "a": short, "b": slanted,
                             "reverse_a": false, "reverse_b": false}}),
    );
    let k = added["dimensions"][0].as_str().unwrap().to_owned();
    let dimension = |d: &Document<MockKernel>| {
        sketch(d, "F3")["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == k.as_str())
            .unwrap()
            .clone()
    };
    let before = dimension(&d)["measured"].as_f64().unwrap();
    // Between the leftward ray and the slanted line: more than 90 degrees.
    assert!(before > std::f64::consts::FRAC_PI_2, "{before}");
    command(
        &mut d,
        json!({"cmd": "sketch.clean_up_projections", "sketch": "F3"}),
    );
    let after = dimension(&d);
    assert_eq!(after["a"], bottom.as_str(), "{after}");
    assert_eq!(after["reverse_a"], true, "{after}");
    assert!(
        close(after["measured"].as_f64().unwrap(), before),
        "{after}"
    );
}

#[test]
fn a_projection_whose_curves_would_no_longer_meet_keeps_its_geometry() {
    // A rounded corner: a line, an arc and a line sharing their ends.
    let mut d = block();
    let arc = Curve3::Conic {
        center: [58.0, 2.0, 20.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: 2.0,
        minor: 2.0,
        start: -std::f64::consts::FRAC_PI_2,
        end: 0.0,
        closed: false,
    };
    *d.kernel().curves.borrow_mut() = vec![
        line([0.0, 0.0], [58.0, 0.0]),
        arc,
        line([60.0, 2.0], [60.0, 40.0]),
    ];
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": FACE},
               "entities": [
                   {"id": "p1", "type": "point", "at": [0.0, 0.0], "fixed": true, "reference": true},
                   {"id": "p2", "type": "point", "at": [58.0, 0.0], "fixed": true, "reference": true},
                   {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
                   {"id": "p4", "type": "point", "at": [58.0, 2.0], "fixed": true, "reference": true},
                   {"id": "p5", "type": "point", "at": [60.0, 2.0], "fixed": true, "reference": true},
                   {"id": "c6", "type": "arc", "center": "p4", "start": "p2", "end": "p5",
                    "fixed": true, "reference": true},
                   {"id": "p7", "type": "point", "at": [60.0, 40.0], "fixed": true, "reference": true},
                   {"id": "c8", "type": "line", "start": "p5", "end": "p7", "fixed": true, "reference": true}],
               "projections": [{"source": FACE, "body": "F2.b0", "by_geometry": true,
                                "entities": ["p1", "p2", "c3", "p4", "p5", "c6", "p7", "c8"]}]}}),
    );
    assert_eq!(feature(&d, "F3")["status"], "ok");
    // The corner is square now: the lines would follow and the arc not,
    // its ends torn from its centre (mitcad#249). Nothing moves, the
    // sketch warns and still solves.
    widen(&mut d, square(80.0, 0));
    let s = sketch(&d, "F3");
    at_close(&s, "p2", [58.0, 0.0]);
    at_close(&s, "p4", [58.0, 2.0]);
    at_close(&s, "p5", [60.0, 2.0]);
    assert_eq!(s["error"], Value::Null, "{s}");
    assert_eq!(s["projections"][0]["kept"], true, "{s}");
    let warnings = s["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{s}");
    assert!(
        warnings[0].as_str().unwrap().contains("no longer match"),
        "{s}"
    );
    assert_eq!(feature(&d, "F3")["status"], "warning");
    // The warning carries the projection's entities (mitcad#264): the
    // Sketch Palette selects them when it is clicked.
    assert_eq!(
        s["warning_entities"],
        json!([["p1", "p2", "c3", "p4", "p5", "c6", "p7", "c8"]]),
        "{s}"
    );
}

#[test]
fn each_warning_carries_the_entities_it_is_about() {
    // A projection that keeps its geometry as a whole: all its entities,
    // also after some of its curves were deleted (mitcad#264).
    let mut d = block();
    *d.kernel().curves.borrow_mut() = square(60.0, 0);
    let corner = |p: [f64; 2], id: &str| json!({"id": id, "type": "point", "at": p, "fixed": true, "reference": true});
    let edge = |id: &str, a: &str, b: &str| json!({"id": id, "type": "line", "start": a, "end": b, "fixed": true, "reference": true});
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": FACE},
               "entities": [
                   corner([0.0, 0.0], "p1"),
                   corner([60.0, 0.0], "p2"),
                   edge("c3", "p1", "p2"),
                   corner([60.0, 40.0], "p4"),
                   edge("c5", "p2", "p4"),
                   corner([0.0, 40.0], "p6"),
                   edge("c7", "p4", "p6"),
                   edge("c8", "p6", "p1")],
               "projections": [{"source": FACE, "body": "F2.b0",
                                "entities": ["p1", "p2", "c3", "p4", "c5", "p6", "c7", "c8"]}]}}),
    );
    assert_eq!(sketch(&d, "F3")["warning_entities"], json!([]));
    command(
        &mut d,
        json!({"cmd": "sketch.remove", "sketch": "F3", "items": ["c8"]}),
    );
    // The face's corner chamfered: another shape, the projection is kept.
    widen(
        &mut d,
        vec![
            line([0.0, 0.0], [80.0, 0.0]),
            line([80.0, 0.0], [80.0, 38.0]),
            line([80.0, 38.0], [78.0, 40.0]),
            line([78.0, 40.0], [0.0, 40.0]),
            line([0.0, 40.0], [0.0, 0.0]),
        ],
    );
    let s = sketch(&d, "F3");
    assert_eq!(s["projections"][0]["kept"], true, "{s}");
    let warnings = s["warnings"].as_array().unwrap();
    let entities = s["warning_entities"].as_array().unwrap();
    assert_eq!(warnings.len(), entities.len(), "{s}");
    let at = warnings
        .iter()
        .position(|w| w.as_str().unwrap().contains("no longer match"))
        .unwrap_or_else(|| panic!("no kept warning in {s}"));
    assert_eq!(entities[at], s["projections"][0]["entities"], "{s}");
    assert!(!entities[at].as_array().unwrap().is_empty(), "{s}");
    assert!(
        !entities[at].as_array().unwrap().contains(&json!("c8")),
        "{s}"
    );
}

#[test]
fn a_projection_of_an_earlier_version_follows_only_the_same_shape() {
    // As files of earlier versions and imports record it: no
    // `by_geometry`. Two of the face's edges, as the face lists them.
    let mut d = block();
    *d.kernel().curves.borrow_mut() = square(60.0, 0);
    let def = |entities: Value| {
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": FACE},
               "entities": entities,
               "projections": [{"source": FACE, "body": "F2.b0",
                                "entities": ["p1", "p2", "c3", "p4", "c5", "p6", "c7", "c8"]}]}})
    };
    let corner = |p: [f64; 2], id: &str| json!({"id": id, "type": "point", "at": p, "fixed": true, "reference": true});
    let edge = |id: &str, a: &str, b: &str| json!({"id": id, "type": "line", "start": a, "end": b, "fixed": true, "reference": true});
    command(
        &mut d,
        def(json!([
            corner([0.0, 0.0], "p1"),
            corner([60.0, 0.0], "p2"),
            edge("c3", "p1", "p2"),
            corner([60.0, 40.0], "p4"),
            edge("c5", "p2", "p4"),
            corner([0.0, 40.0], "p6"),
            edge("c7", "p4", "p6"),
            edge("c8", "p6", "p1")
        ])),
    );
    assert!(
        sketch(&d, "F3")["projections"][0]
            .get("by_geometry")
            .is_none()
    );
    // The same shape wider: it follows, as in earlier versions.
    widen(&mut d, square(80.0, 0));
    let s = sketch(&d, "F3");
    at_close(&s, "p2", [80.0, 0.0]);
    at_close(&s, "p4", [80.0, 40.0]);
    assert_eq!(s["warnings"], json!([]), "{s}");
    // A chamfer on a corner: another shape. Following by geometry would
    // move it; a projection of an earlier version keeps the geometry the
    // file stores and warns (mitcad#249).
    *d.kernel().curves.borrow_mut() = vec![
        line([0.0, 0.0], [90.0, 0.0]),
        line([90.0, 0.0], [90.0, 38.0]),
        line([90.0, 38.0], [88.0, 40.0]),
        line([88.0, 40.0], [0.0, 40.0]),
        line([0.0, 40.0], [0.0, 0.0]),
    ];
    command(
        &mut d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 90}),
    );
    let s = sketch(&d, "F3");
    at_close(&s, "p2", [60.0, 0.0]);
    at_close(&s, "p4", [60.0, 40.0]);
    assert_eq!(s["projections"][0]["kept"], true, "{s}");
    assert!(
        s["warnings"][0]
            .as_str()
            .unwrap()
            .contains("no longer match"),
        "{s}"
    );
    // Project makes projections that follow by geometry.
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    assert_eq!(sketch(&d, "F3")["projections"][1]["by_geometry"], true);
}

// Clean Up Stale Projections (mitcad#252).

/// The top face narrower (from x = 10) with a chamfer on the corner at
/// (60, 40): an edge more.
fn narrower() -> Vec<Curve3> {
    vec![
        line([10.0, 0.0], [60.0, 0.0]),
        line([60.0, 0.0], [60.0, 38.0]),
        line([60.0, 38.0], [58.0, 40.0]),
        line([58.0, 40.0], [10.0, 40.0]),
        line([10.0, 40.0], [10.0, 0.0]),
    ]
}

/// A sketch on the top face with the face's outline projected as an
/// earlier version or an import records it (no `by_geometry`), and
/// `more` entities and `constraints` of the user's.
fn with_an_old_outline(more: Value, constraints: Value) -> Document<MockKernel> {
    let mut d = block();
    *d.kernel().curves.borrow_mut() = square(60.0, 0);
    let corner = |p: [f64; 2], id: &str| json!({"id": id, "type": "point", "at": p, "fixed": true, "reference": true});
    let edge = |id: &str, a: &str, b: &str| json!({"id": id, "type": "line", "start": a, "end": b, "fixed": true, "reference": true});
    let mut entities = vec![
        corner([0.0, 0.0], "p1"),
        corner([60.0, 0.0], "p2"),
        edge("c3", "p1", "p2"),
        corner([60.0, 40.0], "p4"),
        edge("c5", "p2", "p4"),
        corner([0.0, 40.0], "p6"),
        edge("c7", "p4", "p6"),
        edge("c8", "p6", "p1"),
    ];
    entities.extend(more.as_array().unwrap().iter().cloned());
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": FACE},
               "entities": entities, "constraints": constraints,
               "projections": [{"source": FACE, "body": "F2.b0",
                                "entities": ["p1", "p2", "c3", "p4", "c5", "p6", "c7", "c8"]}]}}),
    );
    d
}

/// The face becomes narrower with an edge more: the old outline keeps its
/// geometry, and all its curves are stale.
fn narrow(d: &mut Document<MockKernel>) {
    *d.kernel().curves.borrow_mut() = narrower();
    let changed = command(
        d,
        json!({"cmd": "set_parameter", "name": "d1", "value": 70}),
    );
    assert_eq!(changed["error"], Value::Null, "{changed}");
    let s = sketch(d, "F3");
    assert_eq!(s["projections"][0]["kept"], true, "{s}");
    assert_eq!(
        s["projections"][0]["stale"],
        json!(["c3", "c5", "c7", "c8"]),
        "{s}"
    );
}

/// The key of the sketch's region of this area.
fn region_of_area(s: &Value, area: f64) -> String {
    s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| close(r["area"].as_f64().unwrap(), area))
        .unwrap_or_else(|| panic!("no region of {area} in {s}"))["key"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// An extrude of the sketch F3's region `key`; its uid.
fn extrude(d: &mut Document<MockKernel>, key: &str) -> String {
    let added = command(
        d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": key}],
               "extent": {"type": "distance", "distance": 5}, "operation": "join"}}),
    );
    added["uid"].as_str().unwrap().to_owned()
}

fn has_region(s: &Value, key: &str, area: f64) -> bool {
    s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["key"] == key && close(r["area"].as_f64().unwrap(), area))
}

#[test]
fn the_stale_clean_up_removes_the_old_outline_and_trims_what_the_profile_uses() {
    // The old outline of the face, which then changes shape (narrower, an
    // edge more) and is projected again; an extrude uses the region of the
    // new outline, named by the old curves where they lie on the new ones.
    let mut d = with_an_old_outline(json!([]), json!([]));
    narrow(&mut d);
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    // 50 x 40 less the chamfer.
    let key = region_of_area(&sketch(&d, "F3"), 1998.0);
    assert!(key.contains("c3[") && key.contains("c7["), "{key}");
    let extrusion = extrude(&mut d, &key);
    let cleaned = command(
        &mut d,
        json!({"cmd": "sketch.clean_up_stale_projections", "sketch": "F3"}),
    );
    // The old outline's left edge goes; its bottom, right and top edges
    // bound the region and are trimmed to it (the old corner at (60, 40)
    // parted); the old projection is unlinked.
    let face = FACE.parse::<TopoName>().unwrap().to_string();
    let mut stale = cleaned["stale"].clone();
    let preview = stale.as_object_mut().unwrap().remove("preview").unwrap();
    assert_eq!(
        stale,
        json!({"removed": ["c8"], "trimmed": ["c3", "c5", "c7"], "unlinked": [face],
               "kept": [{"curve": "c3", "why": "region"}, {"curve": "c5", "why": "region"},
                        {"curve": "c7", "why": "region"}]}),
        "{cleaned}"
    );
    // What a preview shows: the left edge as it was, and the pieces cut off
    // the bottom (its left end), the right (its top) and the top (both).
    let near = |p: &Value, q: [f64; 2]| {
        close(p[0].as_f64().unwrap(), q[0]) && close(p[1].as_f64().unwrap(), q[1])
    };
    let removed = preview["removed"].as_array().unwrap();
    assert_eq!(removed.len(), 1, "{cleaned}");
    assert_eq!(
        (&removed[0]["curve"], &removed[0]["type"]),
        (&json!("c8"), &json!("line"))
    );
    let g = &removed[0]["geometry"];
    assert!(
        near(&g["start"], [0.0, 40.0]) && near(&g["end"], [0.0, 0.0]),
        "{cleaned}"
    );
    let cut = preview["cut"].as_array().unwrap();
    let expected = [
        ("c3", [0.0, 0.0], [10.0, 0.0]),
        ("c5", [60.0, 40.0], [60.0, 38.0]),
        ("c7", [60.0, 40.0], [58.0, 40.0]),
        ("c7", [0.0, 40.0], [10.0, 40.0]),
    ];
    assert_eq!(cut.len(), expected.len(), "{cleaned}");
    for (piece, (line, from, to)) in cut.iter().zip(expected) {
        assert!(
            piece["line"] == line && near(&piece["start"], from) && near(&piece["end"], to),
            "{piece} is not {line} {from:?}-{to:?}"
        );
    }
    let warnings = cleaned["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().starts_with("1 stale curve removed: c8")),
        "{cleaned}"
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("whose name changes")),
        "{cleaned}"
    );
    let s = sketch(&d, "F3");
    assert!(!has(&s, "c8"), "{s}");
    assert_eq!(line_id(&s, [10.0, 0.0], [60.0, 0.0]), "c3", "{s}");
    assert_eq!(line_id(&s, [60.0, 0.0], [60.0, 38.0]), "c5", "{s}");
    assert_eq!(line_id(&s, [58.0, 40.0], [10.0, 40.0]), "c7", "{s}");
    // The profile's region and its name stay; the sketch no longer warns.
    assert!(has_region(&s, &key, 1998.0), "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");
    let projections = s["projections"].as_array().unwrap();
    assert_eq!(projections.len(), 1, "{s}");
    assert_eq!(projections[0]["by_geometry"], true, "{s}");
    assert_eq!(feature(&d, "F3")["status"], "ok");
    assert_eq!(feature(&d, &extrusion)["status"], "ok");
    // Nothing left to clean up; one undo step brings it all back.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.clean_up_stale_projections", "sketch": "F3"}),
    );
    assert!(error.contains("nothing to clean up"), "{error}");
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d, "F3");
    assert!(has(&s, "c8"), "{s}");
    assert_eq!(s["projections"].as_array().unwrap().len(), 2, "{s}");
    assert_eq!(s["projections"][0]["kept"], true, "{s}");
}

#[test]
fn a_stale_line_is_trimmed_to_the_users_line_that_bounds_the_profile() {
    // A line of the user's across the old outline, its ends on the old
    // bottom and top edges; an extrude uses the region right of it.
    let mut d = with_an_old_outline(
        json!([
            {"id": "p9", "type": "point", "at": [20.0, 0.0]},
            {"id": "p10", "type": "point", "at": [20.0, 40.0]},
            {"id": "c11", "type": "line", "start": "p9", "end": "p10"}
        ]),
        json!([
            {"id": "k12", "type": "coincident", "point": "p9", "entity": "c3"},
            {"id": "k13", "type": "coincident", "point": "p10", "entity": "c7"}
        ]),
    );
    let key = region_of_area(&sketch(&d, "F3"), 1600.0);
    let extrusion = extrude(&mut d, &key);
    narrow(&mut d);
    assert_eq!(feature(&d, "F3")["status"], "warning");
    let cleaned = command(
        &mut d,
        json!({"cmd": "sketch.clean_up_stale_projections", "sketch": "F3"}),
    );
    assert_eq!(cleaned["stale"]["removed"], json!(["c8"]), "{cleaned}");
    // The bottom and top edges end at the user's line now; the right edge
    // shares its corners with them and keeps its length.
    assert_eq!(
        cleaned["stale"]["trimmed"],
        json!(["c3", "c7"]),
        "{cleaned}"
    );
    let s = sketch(&d, "F3");
    assert_eq!(line_id(&s, [20.0, 0.0], [60.0, 0.0]), "c3", "{s}");
    assert_eq!(line_id(&s, [60.0, 40.0], [20.0, 40.0]), "c7", "{s}");
    assert_eq!(line_id(&s, [60.0, 0.0], [60.0, 40.0]), "c5", "{s}");
    assert!(has_region(&s, &key, 1600.0), "{s}");
    assert_eq!(s["projections"], json!([]), "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");
    assert_eq!(feature(&d, &extrusion)["status"], "ok");
}

#[test]
fn a_projection_after_an_earlier_feature_reads_the_body_as_it_left_it() {
    // mitcad#998: an import's projection of a face as F2 left its body,
    // though F3 takes the body away before the sketch.
    let mut d = block();
    *d.kernel().curves.borrow_mut() = square(60.0, 0);
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "remove_body", "bodies": ["F2.b0"]}}),
    );
    let def = |after: &str| {
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
               "entities": [
                   {"id": "p1", "type": "point", "at": [0.0, 0.0], "fixed": true, "reference": true},
                   {"id": "p2", "type": "point", "at": [60.0, 0.0], "fixed": true, "reference": true},
                   {"id": "p3", "type": "point", "at": [60.0, 40.0], "fixed": true, "reference": true},
                   {"id": "c4", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
                   {"id": "c5", "type": "line", "start": "p2", "end": "p3", "fixed": true, "reference": true}],
               "projections": [{"source": FACE, "body": "F2.b0", "by_geometry": true,
                                "after": after, "entities": ["p1", "p2", "p3", "c4", "c5"]}]}})
    };
    command(&mut d, def("F2"));
    assert_eq!(feature(&d, "F4")["status"], "ok", "{}", feature(&d, "F4"));
    // Only a feature before the sketch.
    let error = rejected(&mut d, def("F9"));
    assert!(error.contains("F9"), "{error}");
    let s = sketch(&d, "F4");
    assert_eq!(s["projections"][0]["after"], "F2", "{s}");
    assert_eq!(s["warnings"], json!([]), "{s}");
    // It follows the face as F2 makes it.
    widen(&mut d, square(80.0, 0));
    let s = sketch(&d, "F4");
    at_close(&s, "p2", [80.0, 0.0]);
    at_close(&s, "p3", [80.0, 40.0]);
    assert_eq!(feature(&d, "F4")["status"], "ok");
    // The design file keeps it.
    let saved = d.to_json();
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert!(file.to_string().contains("\"after\":\"F2\""), "{saved}");
}
