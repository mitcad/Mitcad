// SPDX-License-Identifier: MIT
//! A length to a picked face (mitcad#1630), with the mock kernel: uprights
//! on bases at different heights end level with the first one's top face,
//! follow it when its length changes, end an offset past it, end at a
//! construction plane, are edited to the mode, and are refused with the
//! reason when the plane runs along the part or lies behind its start.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::features::bound_length::BoundMode;
use mitcad_model::testing::MockKernel;
use mitcad_model_tests_support::test_library::*;

/// A part's length as the design has it now.
fn length_of(doc: &Document<MockKernel>, p: &Value) -> f64 {
    doc.part_info(component(p)).unwrap().length.unwrap().1
}

/// The height of a part's top: its start's height plus its length (the
/// uprights stand along +Z).
fn top_of(doc: &Document<MockKernel>, p: &Value) -> f64 {
    placed(doc, occurrence(p)).translation[2] + length_of(doc, p)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// The top face of a placed upright (its end B), as a length's `face`.
fn top_face(doc: &Document<MockKernel>, p: &Value) -> Value {
    let part = derived(doc, component(p));
    json!({"occurrence": p["occurrence"],
           "geometry": {"body": format!("{part}.b0"), "face": format!("{part}:end_b")}})
}

/// An upright standing at `at`, its length to `face` with `offset`.
fn upright_to(doc: &mut Document<MockKernel>, at: [f64; 3], face: Value, offset: f64) -> Value {
    place(
        doc,
        json!({"part": {"source": "S1"}, "at": {"point": at},
               "length": {"mode": "to_face", "face": face, "offset": offset}}),
    )
}

/// The second upright's bound length.
fn bound_def(
    doc: &Document<MockKernel>,
    p: &Value,
) -> mitcad_model::features::bound_length::BoundLengthDef {
    let uid = doc.part_info(component(p)).unwrap().record.bound[0];
    let FeatureDef::BoundLength(b) = &doc.feature(uid).unwrap().def else {
        panic!("a bound length");
    };
    b.clone()
}

#[test]
fn an_upright_ends_level_with_the_first_one_s_top_and_follows_it() {
    let mut doc = design();
    // The first upright on the ground, 500 long; the second on a base
    // 100 higher, beside it: its axis never runs into the first one.
    let first_upright = first(&mut doc, "p30", [0.0; 3], 500.0);
    let face = top_face(&doc, &first_upright);
    let second = upright_to(&mut doc, [200.0, 0.0, 100.0], face.clone(), 0.0);
    all_ok(&doc);
    assert!(
        near(length_of(&doc, &second), 400.0),
        "{}",
        length_of(&doc, &second)
    );
    assert!(near(top_of(&doc, &first_upright), top_of(&doc, &second)));
    assert_eq!(second["bound"].as_array().unwrap().len(), 1);
    let b = bound_def(&doc, &second);
    assert_eq!(b.mode, BoundMode::ToFace);
    assert!(doc.parameters().is_reference(b.parameter));
    // The parts query names the mode and the offset.
    let parts = query(&doc, json!({"query": "build_parts"}));
    let row = parts["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["component"] == second["component"])
        .unwrap()
        .clone();
    assert_eq!(row["bound"]["mode"], "to_face", "{row}");
    assert!(near(row["bound"]["add"].as_f64().unwrap(), 0.0), "{row}");

    // The first one longer: the second's top moves with it.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": first_upright["component"], "length": 600}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(
        near(length_of(&doc, &second), 500.0),
        "{}",
        length_of(&doc, &second)
    );
    assert!(near(top_of(&doc, &second), 600.0));

    // An offset of 10 mm ends it 10 mm above the face.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": second["component"],
               "length": {"mode": "to_face", "face": face, "offset": 10}}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(
        near(length_of(&doc, &second), 510.0),
        "{}",
        length_of(&doc, &second)
    );
    assert!(near(
        top_of(&doc, &second),
        top_of(&doc, &first_upright) + 10.0
    ));
    let parts = query(&doc, json!({"query": "build_parts"}));
    let row = parts["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["component"] == second["component"])
        .unwrap()
        .clone();
    assert!(near(row["bound"]["add"].as_f64().unwrap(), 10.0), "{row}");

    // Placed with the offset from the start: 10 above, and the same with
    // the binding's own form.
    let first_top = top_face(&doc, &first_upright);
    let third = upright_to(&mut doc, [400.0, 0.0, 50.0], first_top.clone(), 10.0);
    all_ok(&doc);
    assert!(
        near(top_of(&doc, &third), 610.0),
        "{}",
        top_of(&doc, &third)
    );
    let fourth = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": {"point": [600.0, 0.0, 0.0]},
               "length": {"bind": first_top, "mode": "to_face", "add": -20}}),
    );
    all_ok(&doc);
    assert!(
        near(top_of(&doc, &fourth), 580.0),
        "{}",
        top_of(&doc, &fourth)
    );
}

#[test]
fn an_upright_ends_at_a_construction_plane() {
    let mut doc = design();
    let upright = first(&mut doc, "p30", [0.0; 3], 500.0);
    let plane = command(
        &mut doc,
        json!({"cmd": "add_feature",
               "def": {"type": "construction_plane",
                       "definition": {"type": "offset", "plane": "xy", "distance": 300}}}),
    )
    .unwrap();
    let plane = plane["uid"].as_str().unwrap().to_owned();
    let second = upright_to(
        &mut doc,
        [200.0, 0.0, 100.0],
        json!({"occurrence": "", "geometry": plane}),
        0.0,
    );
    all_ok(&doc);
    assert!(
        near(top_of(&doc, &second), 300.0),
        "{}",
        top_of(&doc, &second)
    );
    // The origin plane XY, the second's start below it: 5 above it.
    let below = upright_to(
        &mut doc,
        [400.0, 0.0, -200.0],
        json!({"occurrence": "", "geometry": "xy"}),
        5.0,
    );
    all_ok(&doc);
    assert!(
        near(length_of(&doc, &below), 205.0),
        "{}",
        length_of(&doc, &below)
    );
    let _ = upright;
}

#[test]
fn a_plane_along_the_part_or_behind_its_start_is_refused() {
    let mut doc = design();
    let upright = first(&mut doc, "p30", [0.0; 3], 500.0);
    let face = top_face(&doc, &upright);
    let count = doc.timeline().len();
    // Lying along X: the top face's plane runs along its axis.
    let along = command(
        &mut doc,
        json!({"cmd": "build_place", "part": {"source": "S1"},
               "at": {"point": [200.0, 0.0, 100.0], "direction": [1.0, 0.0, 0.0]},
               "length": {"mode": "to_face", "face": face, "offset": 0}}),
    )
    .unwrap_err();
    assert!(along.contains("runs along the profile"), "{along}");
    // Standing above the top: the face is behind its start.
    let behind = command(
        &mut doc,
        json!({"cmd": "build_place", "part": {"source": "S1"},
               "at": {"point": [200.0, 0.0, 700.0]},
               "length": {"mode": "to_face", "face": face, "offset": 0}}),
    )
    .unwrap_err();
    assert!(behind.contains("behind the profile's start"), "{behind}");
    // An offset that ends it before its start.
    let short = command(
        &mut doc,
        json!({"cmd": "build_place", "part": {"source": "S1"},
               "at": {"point": [200.0, 0.0, 450.0]},
               "length": {"mode": "to_face", "face": face, "offset": -60}}),
    )
    .unwrap_err();
    assert!(short.contains("at or before its start"), "{short}");
    // `face` goes with to_face only.
    let other = command(
        &mut doc,
        json!({"cmd": "build_place", "part": {"source": "S1"},
               "at": {"point": [200.0, 0.0, 100.0]},
               "length": {"mode": "face", "face": face}}),
    )
    .unwrap_err();
    assert!(other.contains("to_face"), "{other}");
    // Nothing was added.
    assert_eq!(doc.timeline().len(), count);
}
