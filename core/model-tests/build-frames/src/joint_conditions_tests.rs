// SPDX-License-Identifier: MIT
//! Joint conditions of profiles along sketch lines (mitcad#2200) with the
//! mock kernel: a rectangle placed with Miter, Butt and Square (`true` and
//! `false` still read), an end short of and one into another member's side
//! set to Near Side and Far Side in Ends and following that member's line,
//! an L changed in Ends either way, a T whose stem was placed first, and a
//! box frame of twelve 3D-sketch lines with no overlapping members.

use std::f64::consts::FRAC_PI_4;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

/// The test library's `p30`: a 30 mm square section centred on its axis.
const HALF: f64 = 15.0;

fn sketch_def(points: &[[f64; 2]], lines: &[(usize, usize)]) -> Value {
    let mut entities = Vec::new();
    for (i, p) in points.iter().enumerate() {
        entities.push(json!({"id": format!("p{}", i + 1), "type": "point", "at": p}));
    }
    let first = points.len() + 1;
    for (k, (a, b)) in lines.iter().enumerate() {
        entities.push(json!({"id": format!("c{}", first + k), "type": "line",
                             "start": format!("p{a}"), "end": format!("p{b}")}));
    }
    json!({"type": "sketch", "plane": "xy", "entities": entities})
}

/// A sketch on XY with points `p<n>` and lines `c<n>` after them; its uid.
fn sketch(doc: &mut Document<MockKernel>, points: &[[f64; 2]], lines: &[(usize, usize)]) -> String {
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": sketch_def(points, lines)}),
    )
    .unwrap_or_else(|e| panic!("sketch: {e}"));
    added["uid"]
        .as_str()
        .or_else(|| added["feature"].as_str())
        .unwrap_or_else(|| panic!("no feature in {added}"))
        .to_owned()
}

const RECTANGLE: [[f64; 2]; 4] = [[0.0, 0.0], [1000.0, 0.0], [1000.0, 600.0], [0.0, 600.0]];
const SIDES: [(usize, usize); 4] = [(1, 2), (2, 3), (3, 4), (4, 1)];

fn line(sketch: &str, curve: usize) -> Value {
    json!({"sketch": sketch, "curve": format!("c{curve}")})
}

fn members(answer: &Value) -> Vec<Value> {
    answer["members"]
        .as_array()
        .unwrap_or_else(|| panic!("no members: {answer}"))
        .iter()
        .map(|m| json!({"component": m}))
        .collect()
}

fn length(doc: &Document<MockKernel>, member: &Value) -> f64 {
    doc.part_info(component(member)).unwrap().length.unwrap().1
}

/// A member's end A (its occurrence's placement) and its axis.
fn axis(doc: &Document<MockKernel>, member: &Value) -> ([f64; 3], [f64; 3]) {
    let occurrence = doc.placed_in_host(component(member)).unwrap();
    let t = placed(doc, *occurrence.last().unwrap());
    (t.translation, t.apply_vector([0.0, 0.0, 1.0]))
}

/// Where a member's end is: A, or B its length along its axis on.
fn end(doc: &Document<MockKernel>, member: &Value, b: bool) -> [f64; 3] {
    let (a, z) = axis(doc, member);
    if !b {
        return a;
    }
    let l = length(doc, member);
    [a[0] + z[0] * l, a[1] + z[1] * l, a[2] + z[2] * l]
}

/// A member's box in the host: its axis from end A over its length, the
/// section `HALF` across it every way (the test profile's).
fn bounds(doc: &Document<MockKernel>, member: &Value) -> ([f64; 3], [f64; 3]) {
    let (a, b) = (end(doc, member, false), end(doc, member, true));
    let (mut min, mut max) = ([0.0; 3], [0.0; 3]);
    for i in 0..3 {
        let along = (b[i] - a[i]).abs() > 1e-6;
        let pad = if along { 0.0 } else { HALF };
        min[i] = a[i].min(b[i]) - pad;
        max[i] = a[i].max(b[i]) + pad;
    }
    (min, max)
}

/// How far two members' boxes overlap: the smallest of the three
/// overlaps; zero or less when they only touch or lie apart.
fn overlap(a: ([f64; 3], [f64; 3]), b: ([f64; 3], [f64; 3])) -> f64 {
    (0..3)
        .map(|i| a.1[i].min(b.1[i]) - a.0[i].max(b.0[i]))
        .fold(f64::INFINITY, f64::min)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// A member's row of `build_parts`.
fn row(doc: &Document<MockKernel>, member: &Value) -> Value {
    query(doc, json!({"query": "build_parts"}))["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["component"] == member["component"])
        .cloned()
        .unwrap()
}

/// The joint an end of a member is in, as `build_end_meets` reports it.
fn joint(doc: &Document<MockKernel>, member: &Value, end: &str) -> Value {
    query(
        doc,
        json!({"query": "build_end_meets", "component": member["component"]}),
    )["ends"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["component"] == member["component"] && e["end"] == end)
        .map(|e| e["joint"].clone())
        .unwrap_or_else(|| panic!("no end {end}"))
}

/// Sets the condition of the joint at an end in one undo step.
fn set(doc: &mut Document<MockKernel>, member: &Value, end: &str, condition: Value) {
    let depth = doc.undo_depth();
    let mut joint = json!({"end": end});
    match condition {
        Value::String(c) => joint["condition"] = json!(c),
        Value::Object(fields) => {
            for (k, v) in fields {
                joint[k] = v;
            }
        }
        other => panic!("{other}"),
    }
    command(
        doc,
        json!({"cmd": "build_edit", "component": member["component"], "joint": joint}),
    )
    .unwrap_or_else(|e| panic!("joint {joint}: {e}"));
    all_ok(doc);
    assert_eq!(doc.undo_depth(), depth + 1, "one undo step");
}

fn rectangle(corners: Value) -> (Document<MockKernel>, Vec<Value>, Value) {
    let mut doc = design();
    let s = sketch(&mut doc, &RECTANGLE, &SIDES);
    let answer = place(
        &mut doc,
        json!({"part": part("p30"),
               "lines": {"curves": [line(&s, 5), line(&s, 6), line(&s, 7), line(&s, 8)],
                         "corners": corners}}),
    );
    all_ok(&doc);
    let m = members(&answer);
    (doc, m, answer)
}

#[test]
fn a_rectangle_with_miter_butt_and_square() {
    // Miter, the default, and `true` as it.
    for corners in [json!("mitre"), json!(true)] {
        let (doc, m, _) = rectangle(corners);
        for (member, line_length) in m.iter().zip([1000.0, 600.0, 1000.0, 600.0]) {
            assert!(near(length(&doc, member), line_length + 2.0 * HALF));
            let ends = &row(&doc, member)["ends"];
            assert!((ends["a"]["angle"].as_f64().unwrap() - FRAC_PI_4).abs() < 1e-9);
            assert!((ends["b"]["angle"].as_f64().unwrap() - FRAC_PI_4).abs() < 1e-9);
        }
        assert!(close(end(&doc, &m[0], false), [-HALF, 0.0, 0.0]));
        // Kept with the joint.
        assert_eq!(joint(&doc, &m[0], "b")["condition"], "mitre");
        assert_eq!(joint(&doc, &m[0], "b")["kind"], "L");
    }
    // Butt: at each corner the line picked first runs through to the outer
    // corner, the other stops at its side.
    let (doc, m, answer) = rectangle(json!("butt"));
    assert!(
        answer["warnings"].as_array().is_none_or(Vec::is_empty),
        "{answer}"
    );
    let expect = [
        // The bottom one runs through at both its corners.
        (1000.0 + 2.0 * HALF, [-HALF, 0.0, 0.0]),
        // The right one stops at the bottom one, runs on at the top.
        (600.0, [1000.0, HALF, 0.0]),
        // The top one stops at the right one, runs on at the left.
        (1000.0, [1000.0 - HALF, 600.0, 0.0]),
        // The left one stops at both.
        (600.0 - 2.0 * HALF, [0.0, 600.0 - HALF, 0.0]),
    ];
    for (member, (l, a)) in m.iter().zip(expect) {
        assert!(
            near(length(&doc, member), l),
            "{} for {l}",
            length(&doc, member)
        );
        assert!(
            close(end(&doc, member, false), a),
            "{:?}",
            end(&doc, member, false)
        );
        let ends = &row(&doc, member)["ends"];
        assert_eq!(ends["a"], Value::Null, "{ends}");
        assert_eq!(ends["b"], Value::Null, "{ends}");
    }
    // No overlap and no gap: each pair of neighbours touches.
    for i in 0..4 {
        let (a, b) = (bounds(&doc, &m[i]), bounds(&doc, &m[(i + 1) % 4]));
        assert!(near(overlap(a, b), 0.0), "{i}: {a:?} {b:?}");
    }
    let report = joint(&doc, &m[1], "a");
    assert_eq!(report["condition"], "butt", "{report}");
    assert_eq!(report["through"], m[0]["component"], "{report}");
    // Square, and `false` as it: the lines' lengths, no cuts.
    for corners in [json!("none"), json!(false)] {
        let (doc, m, _) = rectangle(corners);
        for (member, l) in m.iter().zip([1000.0, 600.0, 1000.0, 600.0]) {
            assert!(near(length(&doc, member), l));
            assert_eq!(row(&doc, member)["ends"]["a"], Value::Null);
        }
        assert_eq!(joint(&doc, &m[0], "b")["condition"], "square");
    }
    // Anything else is refused in words.
    let mut doc = design();
    let s = sketch(&mut doc, &RECTANGLE, &SIDES);
    let refused = command(
        &mut doc,
        json!({"cmd": "build_place", "part": part("p30"),
               "lines": {"curves": [line(&s, 5)], "corners": "lapped"}}),
    )
    .unwrap_err();
    assert!(refused.contains("mitre, butt or none"), "{refused}");
}

#[test]
fn the_preview_shows_which_member_runs_through() {
    let mut doc = design();
    let s = sketch(&mut doc, &RECTANGLE, &SIDES);
    let shown = query(
        &doc,
        json!({"query": "build_preview", "part": part("p30"),
               "lines": {"curves": [line(&s, 5), line(&s, 6), line(&s, 7), line(&s, 8)],
                         "corners": "butt"}}),
    );
    let joints = shown["lines"]["joints"]
        .as_array()
        .unwrap_or_else(|| panic!("{shown}"));
    assert_eq!(joints.len(), 4, "{shown}");
    // In the lines' order of their ends: the bottom one's end A (with the
    // left one), its end B (with the right one), then the right one's end
    // B and the top one's.
    for (j, through) in joints.iter().zip([0, 0, 1, 2]) {
        assert_eq!(j["kind"], "L", "{j}");
        assert_eq!(j["condition"], "butt", "{j}");
        assert_eq!(j["through"], through, "{j}");
    }
}

/// The T design: a member along X from (0, `y`) and four stems coming down
/// to it: end A 3 mm short of its side (c12) and 3 mm into it (c13), end B
/// short (c14) and into (c15); none of them touches its line.
fn tee_points(y: f64) -> Vec<[f64; 2]> {
    vec![
        [0.0, y],
        [1000.0, y],
        [500.0, HALF + 3.0],
        [500.0, 400.0],
        [700.0, HALF - 3.0],
        [700.0, 400.0],
        [300.0, 400.0],
        [300.0, HALF + 3.0],
        [200.0, 400.0],
        [200.0, HALF - 3.0],
    ]
}

const TEE_LINES: [(usize, usize); 5] = [(1, 2), (3, 4), (5, 6), (7, 8), (9, 10)];

#[test]
fn an_end_near_another_member_s_side_goes_to_its_near_or_far_side_and_follows_it() {
    let mut doc = design();
    let s = sketch(&mut doc, &tee_points(0.0), &TEE_LINES);
    let answer = place(
        &mut doc,
        json!({"part": part("p30"),
               "lines": {"curves": [line(&s, 11), line(&s, 12), line(&s, 13), line(&s, 14), line(&s, 15)]}}),
    );
    all_ok(&doc);
    let m = members(&answer);
    // As drawn: no line ends on another one.
    assert!(near(end(&doc, &m[1], false)[1], HALF + 3.0));
    let stems = [(&m[1], "a"), (&m[2], "a"), (&m[3], "b"), (&m[4], "b")];
    for (stem, e) in stems {
        let report = joint(&doc, stem, e);
        assert_eq!(report["kind"], "T", "{report}");
        assert_eq!(report["component"], m[0]["component"], "{report}");
        assert_eq!(report["condition"], "square", "{report}");
        let b = e == "b";
        set(&mut doc, stem, e, json!("near_side"));
        assert!(
            near(end(&doc, stem, b)[1], HALF),
            "{:?}",
            end(&doc, stem, b)
        );
        assert_eq!(joint(&doc, stem, e)["condition"], "near_side");
        set(&mut doc, stem, e, json!("far_side"));
        assert!(
            near(end(&doc, stem, b)[1], -HALF),
            "{:?}",
            end(&doc, stem, b)
        );
        assert_eq!(joint(&doc, stem, e)["condition"], "far_side");
    }
    // The member they meet moved 50 up with its line: the ends follow.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": s, "def": sketch_def(&tee_points(50.0), &TEE_LINES)}),
    )
    .unwrap_or_else(|e| panic!("edit: {e}"));
    all_ok(&doc);
    for (stem, e) in stems {
        let y = end(&doc, stem, e == "b")[1];
        assert!(near(y, 50.0 - HALF), "{y}");
    }
    // Undone: back at the far side where it was.
    assert!(doc.undo().is_some());
    for (stem, e) in stems {
        let y = end(&doc, stem, e == "b")[1];
        assert!(near(y, -HALF), "{y}");
    }
    // Square takes the treatment away: as drawn.
    set(&mut doc, &m[1], "a", json!("square"));
    assert!(near(end(&doc, &m[1], false)[1], HALF + 3.0));
    set(&mut doc, &m[4], "b", json!("square"));
    assert!(near(end(&doc, &m[4], true)[1], HALF - 3.0));
    // An L's condition is refused at a T, in words.
    let refused = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": m[2]["component"],
               "joint": {"end": "a", "condition": "butt"}}),
    )
    .unwrap_err();
    assert!(
        refused.contains("a T takes Near Side, Far Side or Square"),
        "{refused}"
    );
}

#[test]
fn an_l_placed_with_miter_changes_to_butt_either_way_and_square_and_back() {
    let (mut doc, m, _) = rectangle(json!("mitre"));
    // The corner at (1000, 0): the bottom member's end B, the right one's A.
    let (bottom, right) = (&m[0], &m[1]);
    // Butt: the one placed first runs through.
    set(&mut doc, bottom, "b", json!("butt"));
    assert!(near(end(&doc, bottom, true)[0], 1000.0 + HALF));
    assert!(near(end(&doc, right, false)[1], HALF));
    assert_eq!(row(&doc, bottom)["ends"]["b"], Value::Null);
    assert_eq!(row(&doc, right)["ends"]["a"], Value::Null);
    let report = joint(&doc, right, "a");
    assert_eq!(report["condition"], "butt", "{report}");
    assert_eq!(report["through"], bottom["component"], "{report}");
    // Swapped: the right one runs through.
    set(
        &mut doc,
        bottom,
        "b",
        json!({"condition": "butt", "through": right["component"]}),
    );
    assert!(near(end(&doc, bottom, true)[0], 1000.0 - HALF));
    assert!(near(end(&doc, right, false)[1], -HALF));
    assert_eq!(joint(&doc, bottom, "b")["through"], right["component"]);
    // No overlap, no gap.
    assert!(near(
        overlap(bounds(&doc, bottom), bounds(&doc, right)),
        0.0
    ));
    // Square: both as drawn.
    set(&mut doc, right, "a", json!("square"));
    assert!(near(end(&doc, bottom, true)[0], 1000.0));
    assert!(near(end(&doc, right, false)[1], 0.0));
    assert_eq!(joint(&doc, bottom, "b")["condition"], "square");
    // Miter again: both to the outer corner, cut at 45 degrees.
    set(&mut doc, bottom, "b", json!("mitre"));
    assert!(near(end(&doc, bottom, true)[0], 1000.0 + HALF));
    assert!(near(end(&doc, right, false)[1], -HALF));
    let cut = row(&doc, right)["ends"]["a"]["angle"].as_f64().unwrap();
    assert!((cut - FRAC_PI_4).abs() < 1e-9);
    assert_eq!(joint(&doc, bottom, "b")["condition"], "mitre");
    // Each change undoes on its own: back to square.
    assert!(doc.undo().is_some());
    assert!(near(end(&doc, bottom, true)[0], 1000.0));
    // The shared miter command reads as Miter: kept with the joint.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": bottom["component"], "shared_mitre": "b"}),
    )
    .unwrap_or_else(|e| panic!("shared miter: {e}"));
    assert_eq!(joint(&doc, bottom, "b")["condition"], "mitre");
    assert!(near(end(&doc, right, false)[1], -HALF));
}

#[test]
fn a_t_whose_stem_was_placed_first_needs_no_reordering() {
    let mut doc = design();
    let s = sketch(
        &mut doc,
        &[[0.0, 0.0], [1000.0, 0.0], [500.0, 0.0], [500.0, 400.0]],
        &[(1, 2), (3, 4)],
    );
    // The stem placed on its own first, then the member it meets.
    let stem = members(&place(
        &mut doc,
        json!({"part": part("p30"), "lines": {"curves": [line(&s, 6)]}}),
    ))
    .remove(0);
    let through = members(&place(
        &mut doc,
        json!({"part": part("p30"), "lines": {"curves": [line(&s, 5)]}}),
    ))
    .remove(0);
    all_ok(&doc);
    let order = |doc: &Document<MockKernel>, member: &Value| {
        let frames = mitcad_model_build::along_of(doc, component(member))
            .unwrap()
            .frames;
        doc.state().position(frames).unwrap()
    };
    let (stem_at, through_at) = (order(&doc, &stem), order(&doc, &through));
    assert!(stem_at < through_at);
    // Near Side: the joint keeps the other member's line, not the member,
    // so nothing binds to a part placed later and nothing moves.
    set(&mut doc, &stem, "a", json!("near_side"));
    assert_eq!(
        (order(&doc, &stem), order(&doc, &through)),
        (stem_at, through_at)
    );
    assert!(near(end(&doc, &stem, false)[1], HALF));
    assert!(near(length(&doc, &stem), 400.0 - HALF));
    // The member it meets turned on its line: the stem follows its side
    // (the section kept anew with the edit).
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": through["component"],
               "along": {"align": "left_above"}}),
    )
    .unwrap_or_else(|e| panic!("align: {e}"));
    all_ok(&doc);
    // Left of the line looking along +X: the section from y 0 to 30.
    assert!(
        near(end(&doc, &stem, false)[1], 2.0 * HALF),
        "{:?}",
        end(&doc, &stem, false)
    );
}

#[test]
fn a_box_frame_of_twelve_3d_sketch_lines_has_no_overlapping_members() {
    let mut doc = design();
    let created = command(&mut doc, json!({"cmd": "sketch3d.create"}))
        .unwrap_or_else(|e| panic!("3D sketch: {e}"));
    let s = created["uid"].as_str().unwrap().to_owned();
    let (x, y, z) = (800.0, 600.0, 400.0);
    let corners = [
        [0.0, 0.0, 0.0],
        [x, 0.0, 0.0],
        [x, y, 0.0],
        [0.0, y, 0.0],
        [0.0, 0.0, z],
        [x, 0.0, z],
        [x, y, z],
        [0.0, y, z],
    ];
    // The bottom, the top, then the uprights.
    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    let mut curves = Vec::new();
    for (a, b) in edges {
        let made = command(
            &mut doc,
            json!({"cmd": "sketch3d.add_line", "sketch": s,
                   "start": corners[a], "end": corners[b]}),
        )
        .unwrap_or_else(|e| panic!("line: {e}"));
        // `made`: the line (a list of one).
        let curve = match &made["made"] {
            Value::Array(list) => list[0].clone(),
            one => one.clone(),
        };
        curves.push(json!({"sketch": s, "curve": curve}));
    }
    let answer = place(
        &mut doc,
        json!({"part": part("p30"), "lines": {"curves": curves}}),
    );
    all_ok(&doc);
    assert!(
        answer["warnings"].as_array().is_none_or(Vec::is_empty),
        "{answer}"
    );
    let m = members(&answer);
    assert_eq!(m.len(), 12);
    // Every pair apart or touching; at each corner the members meet.
    for i in 0..12 {
        for j in i + 1..12 {
            let (a, b) = (bounds(&doc, &m[i]), bounds(&doc, &m[j]));
            assert!(overlap(a, b) <= 1e-6, "{i} and {j} overlap: {a:?} {b:?}");
        }
    }
    // The bottom's first member runs through to the outer corner at the
    // origin; the upright there stops on top of it.
    assert!(close(end(&doc, &m[0], false), [-HALF, 0.0, 0.0]));
    assert!(close(end(&doc, &m[8], false), [0.0, 0.0, HALF]));
}
