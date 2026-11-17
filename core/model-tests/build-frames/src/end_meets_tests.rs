// SPDX-License-Identifier: MIT
//! Ends that meet and their shared miter (mitcad#1940) with the mock
//! kernel: two profiles placed apart in an L (a square corner on an end
//! face, a part on a side near the other's end) meet at their ends, and
//! `build_edit`'s `shared_mitre` makes the shared miter a corner placement
//! makes: both ends cut by half the corner, the outer corners meeting, a
//! part on a side moved through to the other's outer side.

use std::f64::consts::FRAC_PI_4;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

/// The `build_end_meets` answer for one end of a part.
fn end_of(doc: &Document<MockKernel>, part: &Value, end: &str) -> Value {
    let answer = query(
        doc,
        json!({"query": "build_end_meets", "component": part["component"]}),
    );
    answer["ends"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["component"] == part["component"] && e["end"] == end)
        .cloned()
        .unwrap_or_else(|| panic!("no end {end}: {answer}"))
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

fn length(doc: &Document<MockKernel>, part: &Value) -> f64 {
    doc.part_info(component(part)).unwrap().length.unwrap().1
}

#[test]
fn a_square_corner_on_an_end_gets_the_corner_placement_s_shared_miter() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    // A square corner: lying on the upright's end face, flush with its
    // outer side (mitcad#427), no miter.
    let p = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(end_b)")},
                      "direction": [0.0, 1.0, 0.0]},
               "length": 300}),
    );
    all_ok(&doc);
    let end = end_of(&doc, &t, "b");
    let meets = &end["meets"];
    assert_eq!(meets["component"], p["component"], "{end}");
    assert!((number(&meets["angle"]) - FRAC_PI_4).abs() < 1e-9, "{end}");
    // The upright runs on through the part on it to its outer side; the
    // part on the end is at the upright's outer side already.
    assert!((number(&meets["lengthen"]) - 30.0).abs() < 1e-6, "{end}");
    assert!(number(&meets["other_lengthen"]).abs() < 1e-6, "{end}");
    assert_eq!(meets["problem"], Value::Null, "{end}");
    // The four tilts of the panel: towards the part it meets first.
    assert_ne!(meets["tilt"], meets["away"]);
    // Taken: one undo step, both cut, the outer corners meeting.
    let answer = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "shared_mitre": "b"}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(
        answer["changed"]
            .as_array()
            .unwrap()
            .contains(&json!("ends")),
        "{answer}"
    );
    assert!((length(&doc, &t) - 1030.0).abs() < 1e-6);
    assert!((length(&doc, &p) - 300.0).abs() < 1e-6);
    let parts = query(&doc, json!({"query": "build_parts"}));
    for part in parts["parts"].as_array().unwrap() {
        let ends = &part["ends"];
        let cut = if part["component"] == t["component"] {
            &ends["b"]
        } else {
            &ends["a"]
        };
        assert!((number(&cut["angle"]) - FRAC_PI_4).abs() < 1e-9, "{parts}");
    }
    // Still meeting after the cut (the mock kernel's cut bodies keep the
    // cutting tool's box, so their reach is not asked here; the UI test
    // checks the bodies with the real kernel).
    let again = end_of(&doc, &t, "b");
    assert_eq!(again["meets"]["component"], p["component"], "{again}");
    // A mitred L where the two ends meet.
    let junctions = doc.build_junctions(None);
    assert_eq!(junctions.len(), 1, "{junctions:?}");
    assert_eq!(junctions[0].kind, "L");
}

#[test]
fn a_part_on_a_side_near_the_end_runs_through_to_the_outer_side() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    let p = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_x+_1)")},
                      "offset": 970.0},
               "length": 300}),
    );
    all_ok(&doc);
    // Its end on the upright's side: 30 mm through it to its outer side.
    let end = end_of(&doc, &p, "a");
    let meets = &end["meets"];
    assert_eq!(meets["component"], t["component"], "{end}");
    assert_eq!(meets["end"], "b", "{end}");
    assert!((number(&meets["lengthen"]) - 30.0).abs() < 1e-6, "{end}");
    let answer = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": p["component"], "shared_mitre": "a"}),
    )
    .unwrap();
    all_ok(&doc);
    let changed = answer["changed"].as_array().unwrap();
    assert!(
        changed.contains(&json!("moved")) && changed.contains(&json!("length")),
        "{answer}"
    );
    // Its far end stays: 30 mm longer.
    assert!(
        (length(&doc, &p) - 330.0).abs() < 1e-6,
        "{}",
        length(&doc, &p)
    );
    // Still meeting after the cut (the mock kernel's cut bodies keep the
    // cutting tool's box, so their reach is not asked here; the UI test
    // checks the bodies with the real kernel).
    let again = end_of(&doc, &p, "a");
    assert_eq!(again["meets"]["component"], t["component"], "{again}");
    assert_eq!(doc.build_junctions(None)[0].kind, "L");
}

#[test]
fn an_end_that_meets_nothing_says_why() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let end = end_of(&doc, &t, "b");
    assert_eq!(end["meets"], Value::Null);
    assert!(
        end["why"]
            .as_str()
            .is_some_and(|w| w.contains("no other component's end comes within")),
        "{end}"
    );
    // The four tilts' directions, for the panel's arrow.
    assert_eq!(end["leans"].as_object().unwrap().len(), 4);
    let error = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "shared_mitre": "b"}),
    )
    .unwrap_err();
    assert!(error.contains("no other component's end"), "{error}");
}
