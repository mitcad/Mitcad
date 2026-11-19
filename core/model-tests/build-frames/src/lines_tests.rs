// SPDX-License-Identifier: MIT
//! Profiles along sketch lines (mitcad#1965) with the mock kernel: a
//! rectangle of four sketch lines gives four members with shared miters at
//! the corners, a T gives a butt, and a member follows its line when the
//! sketch changes; arcs are refused in words.

use std::f64::consts::FRAC_PI_4;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

/// A sketch on XY with the points and lines given: points `p<n>` at
/// `points[n - 1]`, lines `c<n>` between two of them. Returns its uid.
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

/// Where a member's end A is, its occurrence's placement.
fn end_a(doc: &Document<MockKernel>, member: &Value) -> [f64; 3] {
    let occurrence = doc.placed_in_host(component(member)).unwrap();
    placed(doc, *occurrence.last().unwrap()).translation
}

fn miter(cut: &Value) -> f64 {
    cut["angle"]
        .as_f64()
        .unwrap_or_else(|| panic!("not mitered: {cut}"))
}

#[test]
fn a_rectangle_of_four_lines_gives_four_members_with_shared_miters() {
    let mut doc = design();
    let s = sketch(&mut doc, &RECTANGLE, &SIDES);
    let answer = place(
        &mut doc,
        json!({"part": part("p30"),
               "lines": {"curves": [line(&s, 5), line(&s, 6), line(&s, 7), line(&s, 8)]}}),
    );
    all_ok(&doc);
    assert_eq!(answer["place"], "lines", "{answer}");
    assert_eq!(answer["count"], 4, "{answer}");
    let m = members(&answer);
    // Centred on the lines, each one runs through the corner to the outer
    // corner of the 30 mm section: 15 mm past each end.
    for (member, line_length) in m.iter().zip([1000.0, 600.0, 1000.0, 600.0]) {
        let l = length(&doc, member);
        assert!(
            (l - (line_length + 30.0)).abs() < 1e-6,
            "{l} for {line_length}"
        );
        let ends = &row(&doc, member)["ends"];
        assert!((miter(&ends["a"]) - FRAC_PI_4).abs() < 1e-9, "{ends}");
        assert!((miter(&ends["b"]) - FRAC_PI_4).abs() < 1e-9, "{ends}");
        // It follows its line: placed along it by an edge frame.
        assert!(row(&doc, member)["along"]["line"]["align"] == "center");
    }
    // The bottom line from (0, 0) to (1000, 0): its end A 15 mm before it.
    assert!(
        close(end_a(&doc, &m[0]), [-15.0, 0.0, 0.0]),
        "{:?}",
        end_a(&doc, &m[0])
    );
    // The right one from (1000, 0) up.
    assert!(
        close(end_a(&doc, &m[1]), [1000.0, -15.0, 0.0]),
        "{:?}",
        end_a(&doc, &m[1])
    );
    // Four mitered L's, one undo step.
    let junctions = doc.build_junctions(None);
    assert_eq!(junctions.len(), 4, "{junctions:?}");
    assert!(junctions.iter().all(|j| j.kind == "L"), "{junctions:?}");
    assert!(doc.undo().is_some());
    assert!(doc.build_parts_info().is_empty());
}

#[test]
fn a_whole_sketch_picked_gives_a_member_per_line() {
    let mut doc = design();
    let s = sketch(&mut doc, &RECTANGLE, &SIDES);
    let answer = place(
        &mut doc,
        json!({"part": part("p30"), "lines": {"curves": [{"sketch": s}]}}),
    );
    all_ok(&doc);
    assert_eq!(answer["count"], 4, "{answer}");
    // One line and its chain: the whole closed rectangle.
    let mut doc = design();
    let s = sketch(&mut doc, &RECTANGLE, &SIDES);
    let answer = place(
        &mut doc,
        json!({"part": part("p30"), "lines": {"curves": [line(&s, 5)], "chain": true}}),
    );
    assert_eq!(answer["count"], 4, "{answer}");
}

#[test]
fn a_t_gives_a_butt() {
    let mut doc = design();
    // A line along X and one up from its middle.
    let s = sketch(
        &mut doc,
        &[[0.0, 0.0], [1000.0, 0.0], [500.0, 0.0], [500.0, 400.0]],
        &[(1, 2), (3, 4)],
    );
    let answer = place(
        &mut doc,
        json!({"part": part("p30"), "lines": {"curves": [line(&s, 5), line(&s, 6)]}}),
    );
    all_ok(&doc);
    let m = members(&answer);
    // The through member keeps its line's length, square at both ends.
    assert!((length(&doc, &m[0]) - 1000.0).abs() < 1e-6);
    // The stem stops at its side: 15 mm shorter, its end A on that side.
    assert!(
        (length(&doc, &m[1]) - 385.0).abs() < 1e-6,
        "{}",
        length(&doc, &m[1])
    );
    assert!(
        close(end_a(&doc, &m[1]), [500.0, 15.0, 0.0]),
        "{:?}",
        end_a(&doc, &m[1])
    );
    for member in &m {
        let ends = &row(&doc, member)["ends"];
        assert_eq!(ends["a"], Value::Null, "{ends}");
        assert_eq!(ends["b"], Value::Null, "{ends}");
    }
    assert_eq!(doc.build_junctions(None)[0].kind, "T");
}

#[test]
fn moving_a_sketch_line_moves_and_resizes_its_member() {
    let mut doc = design();
    let s = sketch(&mut doc, &RECTANGLE, &SIDES);
    let answer = place(
        &mut doc,
        json!({"part": part("p30"),
               "lines": {"curves": [line(&s, 5), line(&s, 6), line(&s, 7), line(&s, 8)]}}),
    );
    let m = members(&answer);
    // The right line moved 200 mm out.
    let moved = [[0.0, 0.0], [1200.0, 0.0], [1200.0, 600.0], [0.0, 600.0]];
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": s, "def": sketch_def(&moved, &SIDES)}),
    )
    .unwrap_or_else(|e| panic!("edit: {e}"));
    all_ok(&doc);
    // Its member went with it, the bottom and the top ones longer, the
    // miters kept.
    assert!(
        close(end_a(&doc, &m[1]), [1200.0, -15.0, 0.0]),
        "{:?}",
        end_a(&doc, &m[1])
    );
    assert!((length(&doc, &m[1]) - 630.0).abs() < 1e-6);
    assert!(
        (length(&doc, &m[0]) - 1230.0).abs() < 1e-6,
        "{}",
        length(&doc, &m[0])
    );
    assert!((length(&doc, &m[2]) - 1230.0).abs() < 1e-6);
    let ends = &row(&doc, &m[0])["ends"];
    assert!((miter(&ends["b"]) - FRAC_PI_4).abs() < 1e-9, "{ends}");
}

#[test]
fn alignment_rotation_and_flip_place_the_section_and_the_ends() {
    let mut doc = design();
    let s = sketch(&mut doc, &[[0.0, 0.0], [1000.0, 0.0]], &[(1, 2)]);
    // Resting on the sketch's plane, to the left of the line.
    let answer = place(
        &mut doc,
        json!({"part": part("p30"),
               "lines": {"curves": [line(&s, 3)], "align": "left_above"}}),
    );
    all_ok(&doc);
    let m = members(&answer);
    assert!(
        close(end_a(&doc, &m[0]), [0.0, 15.0, 15.0]),
        "{:?}",
        end_a(&doc, &m[0])
    );
    assert_eq!(row(&doc, &m[0])["along"]["line"]["align"], "left_above");
    // Flipped: end A at the line's end, running back.
    let mut doc = design();
    let s = sketch(&mut doc, &[[0.0, 0.0], [1000.0, 0.0]], &[(1, 2)]);
    let answer = place(
        &mut doc,
        json!({"part": part("p30"), "flip": true, "rotate": "90 deg",
               "lines": {"curves": [line(&s, 3)]}}),
    );
    all_ok(&doc);
    let m = members(&answer);
    assert!(
        close(end_a(&doc, &m[0]), [1000.0, 0.0, 0.0]),
        "{:?}",
        end_a(&doc, &m[0])
    );
    let t = placed(&doc, doc.placed_in_host(component(&m[0])).unwrap()[0]);
    assert!(close(t.apply_vector([0.0, 0.0, 1.0]), [-1.0, 0.0, 0.0]));
    let line_row = &row(&doc, &m[0])["along"]["line"];
    assert_eq!(line_row["flip"], true, "{line_row}");
    assert!((line_row["rotate"].as_f64().unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    // Edited back: unflipped, centered.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": m[0]["component"], "flip": false,
               "rotate": 0, "along": {"align": "center"}}),
    )
    .unwrap_or_else(|e| panic!("edit: {e}"));
    all_ok(&doc);
    assert!(
        close(end_a(&doc, &m[0]), [0.0, 0.0, 0.0]),
        "{:?}",
        end_a(&doc, &m[0])
    );
}

#[test]
fn the_ends_change_afterwards_with_the_ends_and_shared_miter_tools() {
    let mut doc = design();
    let s = sketch(&mut doc, &RECTANGLE, &SIDES);
    let answer = place(
        &mut doc,
        json!({"part": part("p30"), "lines": {"curves": [line(&s, 5), line(&s, 6)],
                                              "corners": false}}),
    );
    all_ok(&doc);
    let m = members(&answer);
    // Left square: the lines' lengths.
    assert!((length(&doc, &m[0]) - 1000.0).abs() < 1e-6);
    // The shared miter afterwards moves the end through its edge frame.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": m[0]["component"], "shared_mitre": "b"}),
    )
    .unwrap_or_else(|e| panic!("shared miter: {e}"));
    all_ok(&doc);
    assert!(
        (length(&doc, &m[0]) - 1015.0).abs() < 1e-6,
        "{}",
        length(&doc, &m[0])
    );
    assert!(
        (length(&doc, &m[1]) - 615.0).abs() < 1e-6,
        "{}",
        length(&doc, &m[1])
    );
    // The end squared again with Ends.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": m[0]["component"], "ends": {"b": null}}),
    )
    .unwrap_or_else(|e| panic!("ends: {e}"));
    assert_eq!(row(&doc, &m[0])["ends"]["b"], Value::Null);
}

#[test]
fn arcs_are_refused_in_words() {
    let mut doc = design();
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
            {"id": "p1", "type": "point", "at": [0, 0]},
            {"id": "p2", "type": "point", "at": [500, 0]},
            {"id": "c3", "type": "line", "start": "p1", "end": "p2"},
            {"id": "p4", "type": "point", "at": [500, 200]},
            {"id": "c5", "type": "circle", "center": "p4", "radius": 50}]}}),
    )
    .unwrap();
    let s = added["uid"].as_str().unwrap().to_owned();
    let refused = command(
        &mut doc,
        json!({"cmd": "build_place", "part": part("p30"), "lines": {"curves": [{"sketch": s}]}}),
    )
    .unwrap_err();
    assert!(refused.contains("straight lines only"), "{refused}");
    assert!(!refused.contains("c5"), "{refused}");
    // Nothing was placed.
    assert!(doc.build_parts_info().is_empty());
    // An origin axis is not a sketch line: `lines` takes sketches only.
    let refused = command(
        &mut doc,
        json!({"cmd": "build_place", "part": part("p30"),
               "lines": {"curves": ["x"]}}),
    )
    .unwrap_err();
    assert!(refused.contains("pick lines of a sketch"), "{refused}");
}
