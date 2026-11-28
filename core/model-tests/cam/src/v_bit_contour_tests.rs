// SPDX-License-Identifier: MIT
//! Contour on the line with the library's engraving cutter and chamfer
//! mill (mitcad#2589, GitHub mitcad/Mitcad#747) on the mock kernel: a
//! sketch curve engraved and a closed profile chamfered without an offset
//! down to the depths asked, the stock simulation cutting their V
//! grooves; outside and inside still refuse them.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The region of a sketch's first rectangle.
const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

/// A 40 x 30 block 10 high, a 12 x 10 rectangle at (10, 10) on a second
/// sketch (F3), a router with a 6 mm flat end mill (1), a 90 degree
/// engraving cutter with a sharp tip (3) and a 90 degree chamfer mill
/// with a 1 mm tip (4), and a setup whose stock's top is the block's (the
/// work zero at its front left corner, so that work X and Y are the
/// design's).
fn engraved() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [10, 10], "width": 12, "height": 10},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": {
            "id": "router",
            "machine": {
                "name": "Bench router",
                "controller": "grbl",
                "travel": {"min": [0, 0, -120], "max": [600, 400, 0]},
                "spindle": {"min_rpm": 8000, "max_rpm": 24000, "spin_up_seconds": 2,
                            "nose_diameter": 50, "nose_length": 80},
                "max_feed": 5000,
                "rapid_feed": 6000,
                "tool_changer": {"kind": "manual"}
            },
            "tools": {"tools": [
                {"number": 1, "name": "6 mm flat end mill", "shape": {"kind": "flat"}, "diameter": 6,
                 "flute_length": 18, "length": 25,
                 "cutting": {"spindle_rpm": 18000, "feed": 1200, "plunge_feed": 300}},
                {"number": 3, "name": "Engraver", "shape": {"kind": "engraver", "angle": 90, "tip_diameter": 0},
                 "diameter": 6, "flute_length": 3, "length": 20,
                 "cutting": {"spindle_rpm": 18000, "feed": 600, "plunge_feed": 200}},
                {"number": 4, "name": "Chamfer", "shape": {"kind": "chamfer", "angle": 90, "tip_diameter": 1},
                 "diameter": 10, "flute_length": 5, "length": 20,
                 "cutting": {"spindle_rpm": 18000, "feed": 600, "plunge_feed": 200}}
            ]}
        }}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"stock": {"kind": "around_bodies", "top": 0}}}),
    );
    doc
}

fn point(value: &Value) -> [f64; 3] {
    [number(&value[0]), number(&value[1]), number(&value[2])]
}

/// The ends of a toolpath's moves below the stock's top that are not
/// rapids (the cuts and the plunges), in work X and Y.
fn cut_ends(op: &Value) -> Vec<[f64; 3]> {
    op["toolpath"]["moves"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["kind"] != json!("rapid"))
        .map(|m| point(&m["to"]))
        .filter(|q| q[2] < -1e-9)
        .collect()
}

/// Whether a point is on the outline of the rectangle from (10, 10) to
/// (22, 20).
fn on_rectangle(q: [f64; 3]) -> bool {
    let inside =
        (10.0 - 1e-6..=22.0 + 1e-6).contains(&q[0]) && (10.0 - 1e-6..=20.0 + 1e-6).contains(&q[1]);
    let on_side = (q[0] - 10.0).abs() < 1e-6
        || (q[0] - 22.0).abs() < 1e-6
        || (q[1] - 10.0).abs() < 1e-6
        || (q[1] - 20.0).abs() < 1e-6;
    inside && on_side
}

#[test]
fn an_engraving_cutter_and_a_chamfer_mill_trace_their_outlines_on_the_line() {
    let mut doc = engraved();
    for operation in [
        // One side of F3's rectangle, an open curve, engraved 1 mm deep.
        json!({"kind": {"type": "contour", "side": "on"}, "tool": 3,
               "inputs": [{"kind": "curves", "sketch": "F3", "curves": ["c1"]}],
               "depths": {"bottom": -1}}),
        // The whole rectangle, closed, chamfered 2 mm deep.
        json!({"kind": {"type": "contour", "side": "on"}, "tool": 4,
               "inputs": [{"kind": "region", "sketch": "F3", "region": R}],
               "depths": {"bottom": -2}}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    for op in generated["setups"][0]["operations"].as_array().unwrap() {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let ops = paths["operations"].as_array().unwrap();
    assert_eq!(ops.len(), 2);
    for (op, bottom) in ops.iter().zip([-1.0, -2.0]) {
        assert!(op.get("error").is_none(), "{op}");
        let ends = cut_ends(op);
        assert!(!ends.is_empty(), "{op}");
        // No offset: every cut on the rectangle's outline.
        for q in &ends {
            assert!(on_rectangle(*q), "off the outline at {q:?}");
        }
        assert!(
            (number(&op["bounds"][0][2]) - bottom).abs() < 1e-9,
            "{}",
            op["bounds"]
        );
    }
    // The chamfer runs round the whole rectangle, no wider.
    let bounds = &ops[1]["bounds"];
    assert!((number(&bounds[0][0]) - 10.0).abs() < 1e-6, "{bounds}");
    assert!((number(&bounds[1][0]) - 22.0).abs() < 1e-6, "{bounds}");
    assert!((number(&bounds[0][1]) - 10.0).abs() < 1e-6, "{bounds}");
    assert!((number(&bounds[1][1]) - 20.0).abs() < 1e-6, "{bounds}");

    // The stock as cut: down to the chamfer's 2 mm, and a V groove's
    // volume (its 1 mm tip and 45 degree sides, 6 mm² across, round the
    // 44 mm outline; the engraving inside it), far less than a flat end
    // mill of the chamfer mill's 10 mm would take (880 mm³).
    let sim = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    assert_eq!(sim["rapid_cuts"], json!([]), "{}", sim["rapid_cuts"]);
    assert!(
        (number(&sim["lowest"]) + 2.0).abs() < 1e-6,
        "{}",
        sim["lowest"]
    );
    let removed = number(&sim["removed"]);
    assert!((200.0..400.0).contains(&removed), "{removed}");
}

#[test]
fn outside_and_inside_refuse_a_v_bit_and_end_mills_still_contour() {
    let mut doc = engraved();
    for side in ["outside", "inside"] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
                "kind": {"type": "contour", "side": side}, "tool": 3,
                "inputs": [{"kind": "region", "sketch": "F3", "region": R}],
                "depths": {"bottom": -1}}}),
        );
    }
    command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "kind": {"type": "contour", "side": "outside"}, "tool": 1,
            "inputs": [{"kind": "region", "sketch": "F3", "region": R}],
            "depths": {"bottom": -1}}}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let ops = generated["setups"][0]["operations"].as_array().unwrap();
    for op in &ops[..2] {
        assert_eq!(op["state"], json!("failed"), "{op}");
        assert!(
            op["error"]
                .as_str()
                .unwrap()
                .contains("only with the tool on the line"),
            "{op}"
        );
    }
    assert_eq!(ops[2]["state"], json!("up_to_date"), "{}", ops[2]);
}
