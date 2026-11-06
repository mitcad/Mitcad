// SPDX-License-Identifier: MIT
//! Project Again (mitcad#1303) with the mock kernel: a linked projection
//! that no longer follows its source is projected anew, its constraints and
//! dimensions carried over to the new curves where they map, and what does
//! not map kept as construction references; projections that only moved
//! or were resized follow on their own.

use std::f64::consts::{FRAC_PI_2, TAU};

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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn sketch(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "sketch", "uid": uid}))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn near(p: &Value, q: [f64; 2]) -> bool {
    close(p[0].as_f64().unwrap(), q[0]) && close(p[1].as_f64().unwrap(), q[1])
}

/// The top face of the block.
const FACE: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";

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

/// The block and a sketch on its top face (F3).
fn block_with_sketch() -> Document<MockKernel> {
    let mut d = block();
    command(
        &mut d,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}}),
    );
    d
}

fn line(a: [f64; 2], b: [f64; 2]) -> Curve3 {
    Curve3::Line {
        start: [a[0], a[1], 20.0],
        end: [b[0], b[1], 20.0],
    }
}

/// An arc on the top face, counter-clockwise from `start` to `end`
/// (radians).
fn arc(center: [f64; 2], radius: f64, start: f64, end: f64) -> Curve3 {
    Curve3::Conic {
        center: [center[0], center[1], 20.0],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: radius,
        minor: radius,
        start,
        end,
        closed: false,
    }
}

fn circle(center: [f64; 2], radius: f64) -> Curve3 {
    // Curve3 is an enum: its variant's fields cannot take `..` from another
    // value, so the arc is made and marked closed.
    let mut whole = arc(center, radius, 0.0, TAU);
    if let Curve3::Conic { closed, .. } = &mut whole {
        *closed = true;
    }
    whole
}

/// A slot around y = 20 of radius 5 whose arcs' centres are at x = `left`
/// and `right`: its bottom line, right arc, top line and left arc.
fn slot(left: f64, right: f64) -> Vec<Curve3> {
    vec![
        line([left, 15.0], [right, 15.0]),
        arc([right, 20.0], 5.0, -FRAC_PI_2, FRAC_PI_2),
        line([right, 25.0], [left, 25.0]),
        arc([left, 20.0], 5.0, FRAC_PI_2, 3.0 * FRAC_PI_2),
    ]
}

/// The model changes (the block gets wider) and the kernel's face curves
/// with it.
fn change(d: &mut Document<MockKernel>, curves: Vec<Curve3>, width: f64) {
    *d.kernel().curves.borrow_mut() = curves;
    let changed = command(
        d,
        json!({"cmd": "set_parameter", "name": "d1", "value": width}),
    );
    assert_eq!(changed["error"], Value::Null, "{changed}");
}

fn entities(s: &Value) -> &Vec<Value> {
    s["entities"].as_array().unwrap()
}

fn entity(s: &Value, id: &str) -> Value {
    entities(s)
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn has(s: &Value, id: &str) -> bool {
    entities(s).iter().any(|e| e["id"] == id)
}

/// The id of the line from `a` to `b` (either way round).
fn line_id(s: &Value, a: [f64; 2], b: [f64; 2]) -> String {
    entities(s)
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

/// The ids of the round curves (`arc` or `circle`) about `center` of
/// `radius`.
fn round_ids(s: &Value, kind: &str, center: [f64; 2], radius: f64) -> Vec<String> {
    entities(s)
        .iter()
        .filter(|e| {
            e["type"] == kind
                && near(&e["geometry"]["center"], center)
                && close(e["geometry"]["radius"].as_f64().unwrap(), radius)
        })
        .map(|e| e["id"].as_str().unwrap().to_owned())
        .collect()
}

fn round_id(s: &Value, kind: &str, center: [f64; 2], radius: f64) -> String {
    let found = round_ids(s, kind, center, radius);
    assert_eq!(
        found.len(),
        1,
        "{kind} at {center:?} r {radius}: {found:?} in {s}"
    );
    found[0].clone()
}

/// The curve a drawing command made.
fn made(result: &Value) -> String {
    if let Some(curve) = result["made"].as_array().and_then(|m| m.first()) {
        return curve.as_str().unwrap().to_owned();
    }
    result["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|id| id.starts_with('c'))
        .unwrap_or_else(|| panic!("no curve made: {result}"))
        .to_owned()
}

fn constrain(d: &mut Document<MockKernel>, constraint: Value) -> String {
    let added = command(
        d,
        json!({"cmd": "sketch.add_constraint", "sketch": "F3", "constraint": constraint}),
    );
    added["constraints"][0].as_str().unwrap().to_owned()
}

fn dimension(d: &mut Document<MockKernel>, dimension: Value, value: f64) -> String {
    let added = command(
        d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F3", "dimension": dimension, "value": value}),
    );
    added["dimensions"][0].as_str().unwrap().to_owned()
}

fn measured(s: &Value, id: &str) -> f64 {
    s["dimensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))["measured"]
        .as_f64()
        .unwrap()
}

fn constraint_ids(s: &Value) -> Vec<String> {
    s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .chain(s["dimensions"].as_array().unwrap())
        .map(|c| c["id"].as_str().unwrap().to_owned())
        .collect()
}

/// What a constraint or a dimension refers to.
fn refers(s: &Value, id: &str) -> Value {
    s["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .chain(s["dimensions"].as_array().unwrap())
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
        .clone()
}

fn warns(s: &Value) -> bool {
    s["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("does not follow"))
}

/// The ring a user drew 3 mm around the projected slot: lines parallel to
/// its lines and arcs concentric with its arcs, each with an offset
/// dimension of 3 mm (as one draws an offset by hand).
struct Ring {
    bottom: String,
    top: String,
    right: String,
    left: String,
    /// The constraints and dimensions, in the order they were added.
    items: Vec<String>,
    offsets: Vec<String>,
}

fn ring(d: &mut Document<MockKernel>, projected: [&str; 4], left: f64, right: f64) -> Ring {
    let [p_bottom, p_right, p_top, p_left] = projected;
    let bottom = made(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [left, 12.0], "end": [right, 12.0]}),
    ));
    let top = made(&command(
        d,
        json!({"cmd": "sketch.add_line", "sketch": "F3", "start": [right, 28.0], "end": [left, 28.0]}),
    ));
    let right_arc = made(&command(
        d,
        json!({"cmd": "sketch.arc", "sketch": "F3", "mode": "center", "center": [right, 20.0],
               "start": [right, 12.0], "end": [right, 28.0]}),
    ));
    let left_arc = made(&command(
        d,
        json!({"cmd": "sketch.arc", "sketch": "F3", "mode": "center", "center": [left, 20.0],
               "start": [left, 28.0], "end": [left, 12.0]}),
    ));
    let mut items = Vec::new();
    let mut offsets = Vec::new();
    for (user, projected) in [(&bottom, p_bottom), (&top, p_top)] {
        items.push(constrain(
            d,
            json!({"type": "parallel", "a": user, "b": projected}),
        ));
        let k = dimension(
            d,
            json!({"type": "line_distance", "a": projected, "b": user}),
            3.0,
        );
        items.push(k.clone());
        offsets.push(k);
    }
    for (user, projected) in [(&right_arc, p_right), (&left_arc, p_left)] {
        items.push(constrain(
            d,
            json!({"type": "concentric", "a": user, "b": projected}),
        ));
        let k = dimension(
            d,
            json!({"type": "concentric_distance", "a": projected, "b": user}),
            3.0,
        );
        items.push(k.clone());
        offsets.push(k);
    }
    Ring {
        bottom,
        top,
        right: right_arc,
        left: left_arc,
        items,
        offsets,
    }
}

#[test]
fn a_slot_whose_arc_comes_back_in_two_pieces_is_projected_again_with_its_offset_ring() {
    // The owner's case: a projected outline of two arcs and two lines with
    // a ring of offset dimensions and parallels on it; the model changes
    // and the source's arc comes back split where its seam now is, so
    // following by geometry would tear it from its lines: kept.
    let mut d = block_with_sketch();
    *d.kernel().curves.borrow_mut() = slot(20.0, 40.0);
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    let s = sketch(&d, "F3");
    let p_bottom = line_id(&s, [20.0, 15.0], [40.0, 15.0]);
    let p_top = line_id(&s, [40.0, 25.0], [20.0, 25.0]);
    let p_right = round_id(&s, "arc", [40.0, 20.0], 5.0);
    let p_left = round_id(&s, "arc", [20.0, 20.0], 5.0);
    let r = ring(
        &mut d,
        [
            p_bottom.as_str(),
            p_right.as_str(),
            p_top.as_str(),
            p_left.as_str(),
        ],
        20.0,
        40.0,
    );
    // A later feature uses the slot's inside (the region of its area; the
    // ring's ends meeting may make a band around it too).
    let s = sketch(&d, "F3");
    let inside = 25.0 * std::f64::consts::PI + 200.0;
    let region = s["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| close(r["area"].as_f64().unwrap(), inside))
        .unwrap_or_else(|| panic!("no region of {inside}: {s}"))["key"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": region}],
               "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}),
    );

    // The slot 10 longer, its right arc in two pieces.
    let mut longer = slot(20.0, 50.0);
    longer.splice(
        1..2,
        [
            arc([50.0, 20.0], 5.0, -FRAC_PI_2, 0.0),
            arc([50.0, 20.0], 5.0, 0.0, FRAC_PI_2),
        ],
    );
    change(&mut d, longer, 70.0);
    let s = sketch(&d, "F3");
    assert_eq!(s["projections"][0]["kept"], true, "{s}");
    assert!(warns(&s), "{s}");
    let timeline = query(&d, json!({"query": "timeline"}));
    let row = timeline["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == "F3")
        .unwrap()
        .clone();
    assert_eq!(row["project_again"], 1, "{row}");

    let again = command(
        &mut d,
        json!({"cmd": "sketch.project_again", "sketch": "F3"}),
    );
    let done = &again["again"][0];
    assert_eq!(done["source"], FACE, "{again}");
    assert_eq!(
        done["left"],
        json!([]),
        "every item is carried over: {again}"
    );
    assert_eq!(done["kept"], json!([]), "no old curve is kept: {again}");
    // The region the extrude uses gains the arc's second piece: its name
    // changes, and the warning says so.
    let warnings = again["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("whose name changes")),
        "{again}"
    );
    let s = sketch(&d, "F3");
    assert!(!warns(&s), "{s}");
    assert!(s["projections"][0].get("kept").is_none(), "{s}");
    // The new curves took the old ones' ids: the lines and the left arc,
    // and the right arc the larger of its pieces.
    assert_eq!(line_id(&s, [20.0, 15.0], [50.0, 15.0]), p_bottom, "{s}");
    assert_eq!(line_id(&s, [50.0, 25.0], [20.0, 25.0]), p_top, "{s}");
    assert_eq!(round_id(&s, "arc", [20.0, 20.0], 5.0), p_left, "{s}");
    let pieces = round_ids(&s, "arc", [50.0, 20.0], 5.0);
    assert_eq!(pieces.len(), 2, "{s}");
    assert!(pieces.contains(&p_right), "{pieces:?} {s}");
    // Every constraint and dimension is there, on the new curves, and the
    // ring follows: its right arc about the new centre, 3 mm out.
    let ids = constraint_ids(&s);
    for k in &r.items {
        assert!(ids.contains(k), "{k} gone: {s}");
    }
    for k in &r.offsets {
        assert!(close(measured(&s, k), 3.0), "{k}: {s}");
    }
    let concentric = refers(&s, &r.items[4]);
    assert!(
        concentric["a"] == p_right || concentric["b"] == p_right,
        "{concentric}"
    );
    let right = entity(&s, &r.right);
    assert!(near(&right["geometry"]["center"], [50.0, 20.0]), "{right}");
    assert!(
        close(right["geometry"]["radius"].as_f64().unwrap(), 8.0),
        "{right}"
    );
    let left = entity(&s, &r.left);
    assert!(near(&left["geometry"]["center"], [20.0, 20.0]), "{left}");
    for (user, y) in [(&r.bottom, 12.0), (&r.top, 28.0)] {
        let g = &entity(&s, user)["geometry"];
        assert!(
            close(g["start"][1].as_f64().unwrap(), y) && close(g["end"][1].as_f64().unwrap(), y),
            "{user}: {g}"
        );
    }
    let timeline = query(&d, json!({"query": "timeline"}));
    assert!(
        timeline["features"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f.get("project_again").is_none()),
        "{timeline}"
    );
    // Nothing is left to project again.
    let error = rejected(
        &mut d,
        json!({"cmd": "sketch.project_again", "sketch": "F3"}),
    );
    assert!(error.contains("nothing to project again"), "{error}");
    // One undo step takes it all back.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d, "F3");
    assert_eq!(s["projections"][0]["kept"], true, "{s}");
    assert!(warns(&s), "{s}");
    assert_eq!(line_id(&s, [20.0, 15.0], [40.0, 15.0]), p_bottom, "{s}");
}

#[test]
fn a_circle_that_becomes_two_arcs_and_two_lines_keeps_what_does_not_map() {
    // A projected circle with a ring circle 3 mm out; the source becomes a
    // slot: the circle finds nothing like it in the source.
    let mut d = block_with_sketch();
    *d.kernel().curves.borrow_mut() = vec![circle([30.0, 20.0], 5.0)];
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    let s = sketch(&d, "F3");
    let projected = round_id(&s, "circle", [30.0, 20.0], 5.0);
    let user = made(&command(
        &mut d,
        json!({"cmd": "sketch.circle", "sketch": "F3", "mode": "center", "center": [30.0, 20.0], "radius": 8}),
    ));
    let concentric = constrain(
        &mut d,
        json!({"type": "concentric", "a": user, "b": projected}),
    );
    let offset = dimension(
        &mut d,
        json!({"type": "concentric_distance", "a": projected, "b": user}),
        3.0,
    );
    change(&mut d, slot(25.0, 35.0), 70.0);
    let s = sketch(&d, "F3");
    assert_eq!(s["projections"][0]["unfollowed"], json!([projected]), "{s}");
    assert!(warns(&s), "{s}");

    let again = command(
        &mut d,
        json!({"cmd": "sketch.project_again", "sketch": "F3", "projection": projected}),
    );
    // Which of the two arcs the circle's constraint would go to is not
    // clear: it stays on the old circle, kept as a construction reference
    // and listed.
    let done = &again["again"][0];
    let mut left: Vec<String> = done["left"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_owned())
        .collect();
    left.sort();
    let mut held = vec![concentric.clone(), offset.clone()];
    held.sort();
    assert_eq!(left, held, "{again}");
    let kept: Vec<&str> = done["kept"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["entity"].as_str().unwrap())
        .collect();
    assert!(kept.contains(&projected.as_str()), "{again}");
    assert!(
        again["warnings"].as_array().unwrap().iter().any(|w| w
            .as_str()
            .unwrap()
            .contains("kept as construction references")),
        "{again}"
    );
    let s = sketch(&d, "F3");
    assert!(!warns(&s), "{s}");
    let old = entity(&s, &projected);
    assert_eq!(old["construction"], true, "{old}");
    assert!(constraint_ids(&s).contains(&concentric), "{s}");
    // The new outline: two arcs and two lines.
    round_id(&s, "arc", [25.0, 20.0], 5.0);
    round_id(&s, "arc", [35.0, 20.0], 5.0);
    line_id(&s, [25.0, 15.0], [35.0, 15.0]);
    line_id(&s, [35.0, 25.0], [25.0, 25.0]);
    // One undo step.
    command(&mut d, json!({"cmd": "undo"}));
    let s = sketch(&d, "F3");
    assert!(warns(&s), "{s}");
    assert_eq!(entity(&s, &projected).get("construction"), None);
}

#[test]
fn a_slot_that_becomes_a_circle_carries_its_arcs_offsets_over_to_the_circle() {
    // The reverse: the slot's arcs' ring arcs go with them to the circle;
    // the parallels and distances of its lines have no line to go to.
    let mut d = block_with_sketch();
    *d.kernel().curves.borrow_mut() = slot(25.0, 35.0);
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    let s = sketch(&d, "F3");
    let p_bottom = line_id(&s, [25.0, 15.0], [35.0, 15.0]);
    let p_top = line_id(&s, [35.0, 25.0], [25.0, 25.0]);
    let p_right = round_id(&s, "arc", [35.0, 20.0], 5.0);
    let p_left = round_id(&s, "arc", [25.0, 20.0], 5.0);
    let r = ring(
        &mut d,
        [
            p_bottom.as_str(),
            p_right.as_str(),
            p_top.as_str(),
            p_left.as_str(),
        ],
        25.0,
        35.0,
    );
    change(&mut d, vec![circle([30.0, 20.0], 5.0)], 70.0);
    let s = sketch(&d, "F3");
    assert!(warns(&s), "{s}");

    let again = command(
        &mut d,
        json!({"cmd": "sketch.project_again", "sketch": "F3"}),
    );
    let done = &again["again"][0];
    // The lines' four items stay on them, kept as construction references.
    let mut left: Vec<String> = done["left"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_owned())
        .collect();
    left.sort();
    let mut lines: Vec<String> = r.items[..4].to_vec();
    lines.sort();
    assert_eq!(left, lines, "{again}");
    let s = sketch(&d, "F3");
    assert!(!warns(&s), "{s}");
    for line in [&p_bottom, &p_top] {
        assert_eq!(entity(&s, line)["construction"], true, "{line}: {s}");
    }
    // The arcs' items are on the circle, which took the lower arc's id;
    // the ring's arcs are about its centre, 3 mm out.
    let new = round_id(&s, "circle", [30.0, 20.0], 5.0);
    let number = |id: &str| id[1..].parse::<u32>().unwrap();
    let (lower, higher) = if number(&p_right) < number(&p_left) {
        (&p_right, &p_left)
    } else {
        (&p_left, &p_right)
    };
    assert_eq!(&new, lower, "{s}");
    assert!(!has(&s, higher), "{s}");
    for k in &r.items[4..] {
        let item = refers(&s, k);
        assert!(item["a"] == new || item["b"] == new, "{item}");
    }
    for k in &r.offsets[2..] {
        assert!(close(measured(&s, k), 3.0), "{k}: {s}");
    }
    for user in [&r.right, &r.left] {
        let g = &entity(&s, user)["geometry"];
        assert!(near(&g["center"], [30.0, 20.0]), "{user}: {g}");
        assert!(close(g["radius"].as_f64().unwrap(), 8.0), "{user}: {g}");
    }
}

#[test]
fn an_imported_projection_whose_source_lists_its_curves_from_another_start_follows() {
    // As an import records it (no by_geometry): the slot's curves in the
    // kernel's order then. The source now lists them from its right arc on
    // and the slot is longer: the same curves, only moved and resized, so
    // it follows on its own (mitcad#1303) and the extrude's region keeps
    // its name.
    let mut d = block();
    *d.kernel().curves.borrow_mut() = slot(20.0, 40.0);
    command(
        &mut d,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": {"face": FACE},
               "entities": [
                   {"id": "p1", "type": "point", "at": [20.0, 15.0], "fixed": true, "reference": true},
                   {"id": "p2", "type": "point", "at": [40.0, 15.0], "fixed": true, "reference": true},
                   {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
                   {"id": "p4", "type": "point", "at": [40.0, 20.0], "fixed": true, "reference": true},
                   {"id": "p5", "type": "point", "at": [40.0, 25.0], "fixed": true, "reference": true},
                   {"id": "c6", "type": "arc", "center": "p4", "start": "p2", "end": "p5", "fixed": true, "reference": true},
                   {"id": "p7", "type": "point", "at": [20.0, 25.0], "fixed": true, "reference": true},
                   {"id": "c8", "type": "line", "start": "p5", "end": "p7", "fixed": true, "reference": true},
                   {"id": "p9", "type": "point", "at": [20.0, 20.0], "fixed": true, "reference": true},
                   {"id": "c10", "type": "arc", "center": "p9", "start": "p7", "end": "p1", "fixed": true, "reference": true}],
               "projections": [{"source": FACE, "body": "F2.b0",
                                "entities": ["p1", "p2", "c3", "p4", "p5", "c6", "p7", "c8", "p9", "c10"]}]}}),
    );
    let s = sketch(&d, "F3");
    assert!(!warns(&s), "{s}");
    let region = s["regions"][0]["key"].as_str().unwrap().to_owned();
    let mut moved = slot(20.0, 50.0);
    moved.rotate_left(1);
    change(&mut d, moved, 70.0);
    let s = sketch(&d, "F3");
    assert!(!warns(&s), "{s}");
    assert!(s["projections"][0].get("kept").is_none(), "{s}");
    assert_eq!(line_id(&s, [20.0, 15.0], [50.0, 15.0]), "c3", "{s}");
    assert_eq!(round_id(&s, "arc", [50.0, 20.0], 5.0), "c6", "{s}");
    assert_eq!(s["regions"][0]["key"], region.as_str(), "{s}");
}

#[test]
fn a_slot_moved_by_half_its_length_and_listed_from_another_start_follows() {
    // Nearest first, its right arc would pair with the new left arc and
    // tear from its lines; laid over the new outline every curve pairs
    // with its own (mitcad#1303).
    let mut d = block_with_sketch();
    *d.kernel().curves.borrow_mut() = slot(20.0, 40.0);
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    let s = sketch(&d, "F3");
    let p_bottom = line_id(&s, [20.0, 15.0], [40.0, 15.0]);
    let p_right = round_id(&s, "arc", [40.0, 20.0], 5.0);
    let region = s["regions"][0]["key"].as_str().unwrap().to_owned();
    let mut moved = slot(35.0, 55.0);
    moved.rotate_left(1);
    change(&mut d, moved, 70.0);
    let s = sketch(&d, "F3");
    assert!(!warns(&s), "{s}");
    assert_eq!(line_id(&s, [35.0, 15.0], [55.0, 15.0]), p_bottom, "{s}");
    assert_eq!(round_id(&s, "arc", [55.0, 20.0], 5.0), p_right, "{s}");
    assert_eq!(s["regions"][0]["key"], region.as_str(), "{s}");
}

/// [`slot`] with its right arc in two pieces, joined at its far end.
fn slot_split_right(left: f64, right: f64) -> Vec<Curve3> {
    let mut curves = slot(left, right);
    curves.splice(
        1..2,
        [
            arc([right, 20.0], 5.0, -FRAC_PI_2, 0.0),
            arc([right, 20.0], 5.0, 0.0, FRAC_PI_2),
        ],
    );
    curves
}

#[test]
fn a_projected_arc_in_two_pieces_follows_its_source_listing_it_in_one() {
    // A projected face outline whose rounded end was two arcs joined where
    // the kernel split it, with an offset ring 3 mm out; the source comes
    // back with that end as one arc, where it was and then moved: the
    // projection follows as it is, its arc in its pieces (mitcad#1325).
    let mut d = block_with_sketch();
    *d.kernel().curves.borrow_mut() = slot_split_right(20.0, 40.0);
    command(
        &mut d,
        json!({"cmd": "sketch.project", "sketch": "F3", "source": FACE, "body": "F2.b0", "linked": true}),
    );
    let s = sketch(&d, "F3");
    let p_bottom = line_id(&s, [20.0, 15.0], [40.0, 15.0]);
    let p_top = line_id(&s, [40.0, 25.0], [20.0, 25.0]);
    let p_left = round_id(&s, "arc", [20.0, 20.0], 5.0);
    let mut p_right = round_ids(&s, "arc", [40.0, 20.0], 5.0);
    p_right.sort();
    assert_eq!(p_right.len(), 2, "{s}");
    let listed = s["projections"][0]["entities"].clone();
    let curves = json!([p_bottom, p_right[0], p_right[1], p_top, p_left]);
    let offset = command(
        &mut d,
        json!({"cmd": "sketch.offset", "sketch": "F3", "curves": curves, "distance": 3}),
    );
    assert_eq!(
        round_ids(&sketch(&d, "F3"), "arc", [40.0, 20.0], 8.0).len(),
        2,
        "{offset}"
    );

    // The same face, its end now one arc.
    change(&mut d, slot(20.0, 40.0), 65.0);
    let s = sketch(&d, "F3");
    assert!(!warns(&s), "{s}");
    assert!(s["projections"][0].get("kept").is_none(), "{s}");
    assert_eq!(s["projections"][0]["entities"], listed, "{s}");
    let mut right = round_ids(&s, "arc", [40.0, 20.0], 5.0);
    right.sort();
    assert_eq!(right, p_right, "{s}");
    let timeline = query(&d, json!({"query": "timeline"}));
    assert!(
        timeline["features"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f.get("project_again").is_none()),
        "{timeline}"
    );

    // The face 10 longer, its end still one arc: the pieces move with it,
    // joined at the same angle, and the offset ring follows.
    change(&mut d, slot(20.0, 50.0), 70.0);
    let s = sketch(&d, "F3");
    assert!(!warns(&s), "{s}");
    assert_eq!(s["projections"][0]["entities"], listed, "{s}");
    assert_eq!(line_id(&s, [20.0, 15.0], [50.0, 15.0]), p_bottom, "{s}");
    assert_eq!(line_id(&s, [50.0, 25.0], [20.0, 25.0]), p_top, "{s}");
    assert_eq!(round_id(&s, "arc", [20.0, 20.0], 5.0), p_left, "{s}");
    let mut right = round_ids(&s, "arc", [50.0, 20.0], 5.0);
    right.sort();
    assert_eq!(right, p_right, "{s}");
    for id in &p_right {
        let g = &entity(&s, id)["geometry"];
        assert!(
            near(&g["start"], [55.0, 20.0]) || near(&g["end"], [55.0, 20.0]),
            "{id}: {g}"
        );
    }
    assert_eq!(round_ids(&s, "arc", [50.0, 20.0], 8.0).len(), 2, "{s}");
}
