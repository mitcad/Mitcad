// SPDX-License-Identifier: MIT
//! Profile Boundary (mitcad#2551), with the mock kernel: a block whose top
//! face has four bosses standing on it (four circles among the face's
//! edges), a sketch made on the face with its outline (the automatic,
//! linked projection, mitcad#2051) and the outline offset 2 mm inwards.
//! The inset is one profile with the bosses' circles inside it
//! (mitcad#2197); `sketch.set_profile_boundary` with the four circles makes
//! them holes in it, and an Extrude of the inset leaves the footprints
//! clear; with two of them only those two. The choice is saved, read
//! again, and still holds when the face's edges move. Without the inset
//! the face-bounded area stays as it was, and a curve the user drew is
//! refused in words.

use std::f64::consts::{PI, TAU};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, FaceName, RegionKey, TopoName};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.query(&value.to_string()).unwrap()).unwrap()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "sketch", "uid": "F3"}))
}

/// The block's region in Sketch1 and its top face.
const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const FACE: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";

/// The bosses' centres on the top face; each is 12 mm across.
const BOSSES: [[f64; 2]; 4] = [[15.0, 10.0], [45.0, 10.0], [15.0, 30.0], [45.0, 30.0]];
const RADIUS: f64 = 6.0;
const TOP: f64 = 20.0;

fn boss_area() -> f64 {
    PI * RADIUS * RADIUS
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-3 * b.abs().max(1.0)
}

/// The top face's edges of a block `width` wide and 40 deep with bosses
/// at `bosses`: the face's boundary (the face-bounded regions), its
/// outline (the projection of the face) and each edge's curve.
fn set_face(kernel: &MockKernel, width: f64, bosses: &[[f64; 2]; 4]) {
    let top: FaceName = FACE.parse().unwrap();
    let region: RegionKey = REGION.parse().unwrap();
    let c = [[0.0, 0.0], [width, 0.0], [width, 40.0], [0.0, 40.0]];
    let mut edges: Vec<(EdgeName, Curve3)> = region
        .segments()
        .enumerate()
        .map(|(i, segment)| {
            let side = FaceName::side(FeatureUid(2), segment.clone());
            let (a, b) = (c[i], c[(i + 1) % 4]);
            (
                EdgeName::new(top.clone(), side),
                Curve3::Line {
                    start: [a[0], a[1], TOP],
                    end: [b[0], b[1], TOP],
                },
            )
        })
        .collect();
    for (i, center) in bosses.iter().enumerate() {
        let boss = FaceName::new(FeatureUid(2), "boss", None);
        edges.push((
            EdgeName::new(top.clone(), boss).with_index(i as u32),
            Curve3::Conic {
                center: [center[0], center[1], TOP],
                normal: [0.0, 0.0, 1.0],
                x_axis: [1.0, 0.0, 0.0],
                major: RADIUS,
                minor: RADIUS,
                start: 0.0,
                end: TAU,
                closed: true,
            },
        ));
    }
    kernel
        .face_boundaries
        .borrow_mut()
        .insert(top.to_string(), edges.clone());
    *kernel.curves.borrow_mut() = edges.iter().map(|(_, c)| c.clone()).collect();
    let mut named = kernel.named_curves.borrow_mut();
    named.clear();
    for (edge, curve) in edges {
        named.insert(TopoName::Edge(edge).to_string(), curve);
    }
}

fn face_kernel() -> MockKernel {
    let kernel = MockKernel::default();
    set_face(&kernel, 60.0, &BOSSES);
    kernel
}

/// A 60 x 40 block 20 high (Sketch1 F1, Extrude1 F2) with the bosses'
/// circles on its top face, and Sketch3 (F3) made on that face with its
/// outline.
fn block_with_sketch() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F1", "region": REGION}],
               "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}),
    );
    set_face(doc.kernel(), 60.0, &BOSSES);
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": FACE}, "project_face": true}),
    );
    doc
}

fn of_type(s: &Value, kind: &str) -> Vec<String> {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == kind)
        .map(|e| e["id"].as_str().unwrap().to_owned())
        .collect()
}

fn entity<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {s}"))
}

/// The outline's circle about `center`.
fn circle_at(s: &Value, center: [f64; 2]) -> String {
    of_type(s, "circle")
        .into_iter()
        .find(|id| {
            let g = &entity(s, id)["geometry"]["center"];
            close(g[0].as_f64().unwrap(), center[0]) && close(g[1].as_f64().unwrap(), center[1])
        })
        .unwrap_or_else(|| panic!("no circle at {center:?}: {s}"))
}

fn regions(s: &Value) -> &Vec<Value> {
    s["regions"].as_array().unwrap()
}

fn area(region: &Value) -> f64 {
    region["area"].as_f64().unwrap()
}

/// The outline offset 2 mm inwards: the inset's lines, and the sketch
/// with it. The chain's inward side is found by the inset's area.
fn inset(doc: &mut Document<MockKernel>) -> Vec<String> {
    let lines = of_type(&sketch(doc), "line");
    assert_eq!(lines.len(), 4, "{}", sketch(doc));
    for distance in [-2.0, 2.0] {
        command(
            doc,
            json!({"cmd": "sketch.offset", "sketch": "F3", "curves": lines, "distance": distance}),
        );
        let s = sketch(doc);
        if regions(&s).iter().any(|r| close(area(r), 56.0 * 36.0)) {
            let o = s["offsets"].as_array().unwrap().last().unwrap();
            return o["results"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_owned())
                .collect();
        }
        command(doc, json!({"cmd": "undo"}));
    }
    panic!("no inset: {}", sketch(doc));
}

/// The inset's region: the one whose outer loop is the inset's lines.
fn inset_region<'a>(s: &'a Value, lines: &[String]) -> &'a Value {
    regions(s)
        .iter()
        .find(|r| {
            let key = r["key"].as_str().unwrap();
            lines.iter().all(|l| key.contains(&format!("{l}[")))
        })
        .unwrap_or_else(|| panic!("no inset region: {s}"))
}

fn loops(region: &Value) -> usize {
    region["loops"].as_array().unwrap().len()
}

fn choose(doc: &mut Document<MockKernel>, curves: &[String], boundary: bool) -> Value {
    command(
        doc,
        json!({"cmd": "sketch.set_profile_boundary", "sketch": "F3", "curves": curves,
               "boundary": boundary}),
    )
}

/// Extrude4 (F4) of the region as a new body; the side faces of its body
/// (the inset's four and one per hole).
fn extrude_sides(doc: &mut Document<MockKernel>, key: &Value) -> usize {
    command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": "F3", "region": key}],
               "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}),
    );
    assert!(
        matches!(
            doc.status(FeatureUid(4)),
            Some(mitcad_model::FeatureStatus::Ok)
        ),
        "{}",
        query(doc, json!({"query": "timeline"}))
    );
    doc.body_shape(BodyUid::new(FeatureUid(4), 0))
        .unwrap()
        .faces
        .iter()
        .filter(|f| f.role == "side")
        .count()
}

#[test]
fn the_chosen_circles_make_holes_in_the_inset_and_its_extrude() {
    let mut doc = block_with_sketch();
    let s = sketch(&doc);
    assert_eq!(s["projections"][0]["automatic"], true, "{s}");
    let circles: Vec<String> = BOSSES.iter().map(|c| circle_at(&s, *c)).collect();
    let lines = inset(&mut doc);
    // As before: the bosses' circles do not split the inset.
    let s = sketch(&doc);
    let plain = inset_region(&s, &lines).clone();
    assert_eq!(loops(&plain), 1, "{s}");
    assert!(close(area(&plain), 56.0 * 36.0), "{s}");
    let result = choose(&mut doc, &circles, true);
    assert_eq!(result["error"], Value::Null, "{result}");
    let s = sketch(&doc);
    let mut chosen: Vec<String> = s["projections"][0]["boundaries"]
        .as_array()
        .unwrap_or_else(|| panic!("no boundaries: {s}"))
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    chosen.sort();
    let mut expected = circles.clone();
    expected.sort();
    assert_eq!(chosen, expected);
    // The circles stay curves of the linked outline.
    assert_eq!(s["projections"].as_array().unwrap().len(), 1, "{s}");
    let holed = inset_region(&s, &lines);
    assert_eq!(holed["key"], plain["key"], "{s}");
    assert_eq!(loops(holed), 5, "{s}");
    assert!(close(area(holed), 56.0 * 36.0 - 4.0 * boss_area()), "{s}");
    // Each footprint is a profile of its own.
    for circle in &circles {
        let key = format!("r{{{circle}}}");
        let disc = regions(&s)
            .iter()
            .find(|r| r["key"] == key.as_str())
            .unwrap_or_else(|| panic!("no {key}: {s}"));
        assert!(close(area(disc), boss_area()), "{s}");
    }
    // Extrude of the inset: four sides and the four holes' walls.
    let key = holed["key"].clone();
    assert_eq!(extrude_sides(&mut doc, &key), 8);
}

#[test]
fn only_the_chosen_circles_make_holes() {
    let mut doc = block_with_sketch();
    let s = sketch(&doc);
    let two = vec![circle_at(&s, BOSSES[0]), circle_at(&s, BOSSES[3])];
    let lines = inset(&mut doc);
    choose(&mut doc, &two, true);
    let s = sketch(&doc);
    let holed = inset_region(&s, &lines);
    assert_eq!(loops(holed), 3, "{s}");
    assert!(close(area(holed), 56.0 * 36.0 - 2.0 * boss_area()), "{s}");
    let key = holed["key"].clone();
    assert_eq!(extrude_sides(&mut doc, &key), 6);
}

#[test]
fn choosing_again_off_gives_the_whole_inset_back() {
    let mut doc = block_with_sketch();
    let s = sketch(&doc);
    let circles: Vec<String> = BOSSES.iter().map(|c| circle_at(&s, *c)).collect();
    let lines = inset(&mut doc);
    choose(&mut doc, &circles, true);
    choose(&mut doc, &circles[..2], false);
    let s = sketch(&doc);
    assert_eq!(loops(inset_region(&s, &lines)), 3, "{s}");
    choose(&mut doc, &circles[2..], false);
    let s = sketch(&doc);
    assert!(s["projections"][0].get("boundaries").is_none(), "{s}");
    assert_eq!(loops(inset_region(&s, &lines)), 1, "{s}");
    // One undo step each: undone, the two are chosen again.
    command(&mut doc, json!({"cmd": "undo"}));
    let s = sketch(&doc);
    assert_eq!(loops(inset_region(&s, &lines)), 3, "{s}");
}

#[test]
fn the_choice_is_saved_and_follows_the_face() {
    let mut doc = block_with_sketch();
    let s = sketch(&doc);
    let circles: Vec<String> = BOSSES.iter().map(|c| circle_at(&s, *c)).collect();
    let lines = inset(&mut doc);
    choose(&mut doc, &circles, true);
    let saved = doc.to_json();
    assert!(saved.contains("\"boundaries\""), "{saved}");
    let mut loaded = Document::from_json(&saved, face_kernel()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    // A design read is evaluated by its first recompute.
    loaded.recompute();
    let s = sketch(&loaded);
    assert_eq!(loops(inset_region(&s, &lines)), 5, "{s}");
    // The block grows 1 mm wider and a boss moves: the outline and its
    // inset follow, and the chosen circles stay holes.
    let mut moved = BOSSES;
    moved[0] = [20.0, 12.0];
    set_face(loaded.kernel(), 61.0, &moved);
    let changed = command(
        &mut loaded,
        json!({"cmd": "set_parameter", "name": "d1", "value": 61}),
    );
    assert_eq!(changed["error"], Value::Null, "{changed}");
    let s = sketch(&loaded);
    assert_eq!(circle_at(&s, moved[0]), circles[0], "{s}");
    assert_eq!(loops(inset_region(&s, &lines)), 5, "{s}");
    let key = format!("r{{{}}}", circles[0]);
    let disc = regions(&s)
        .iter()
        .find(|r| r["key"] == key.as_str())
        .unwrap_or_else(|| panic!("no {key}: {s}"));
    assert!(close(area(disc), boss_area()), "{s}");
}

#[test]
fn without_the_inset_the_face_bounded_area_stays() {
    let mut doc = block_with_sketch();
    let s = sketch(&doc);
    let circles: Vec<String> = BOSSES.iter().map(|c| circle_at(&s, *c)).collect();
    let face = 60.0 * 40.0 - 4.0 * boss_area();
    let leftover = regions(&s)
        .iter()
        .find(|r| close(area(r), face))
        .unwrap_or_else(|| panic!("no face-bounded area: {s}"))
        .clone();
    choose(&mut doc, &circles, true);
    let s = sketch(&doc);
    let after = regions(&s)
        .iter()
        .find(|r| r["key"] == leftover["key"])
        .unwrap_or_else(|| panic!("the face-bounded area went: {s}"));
    assert!(close(area(after), face), "{s}");
    // An Extrude of it takes the face less the footprints.
    let key = after["key"].clone();
    assert_eq!(extrude_sides(&mut doc, &key), 8);
}

#[test]
fn a_curve_the_user_drew_is_refused_in_words() {
    let mut doc = block_with_sketch();
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 20], "diameter": 4}),
    );
    let s = sketch(&doc);
    let drawn = circle_at(&s, [30.0, 20.0]);
    let refused = doc
        .command(
            &json!({"cmd": "sketch.set_profile_boundary", "sketch": "F3", "curves": [drawn]})
                .to_string(),
        )
        .unwrap_or_else(|e| e.to_string());
    assert!(refused.contains("outline"), "{refused}");
    let s = sketch(&doc);
    assert!(s["projections"][0].get("boundaries").is_none(), "{s}");
}
