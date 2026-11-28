// SPDX-License-Identifier: MIT
//! Turning limits and the spun target through the JSON api (mitcad#2629,
//! GitHub mitcad/Mitcad#690) on the mock kernel: the block 40 x 30 x 10
//! turned about the design's Z through its middle (turning_tests.rs). The
//! section through the axis and X sees it 40 mm across; its corners reach
//! 25 mm from the axis at other spindle angles. With the spun target the
//! turned profile and the stock keep the corners, and the roughing stays
//! outside them, while a live tool's hole is drilled from the block as it
//! is; without it the profile is the section's, as before. A finish
//! limited to a stretch along Z cuts only there; limits the wrong way
//! round are refused in words.

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

/// A lathe with a C axis and live tools: a turning tool (1) and an axial
/// drill (5).
fn mill_turn() -> Value {
    json!({
        "id": "mill-turn",
        "machine": {
            "name": "Mill-turn",
            "controller": "fanuc_turning",
            "travel": {"min": [-5, 0, -300], "max": [150, 0, 200]},
            "spindle": {"min_rpm": 50, "max_rpm": 3000, "nose_diameter": 0, "nose_length": 0},
            "max_feed": 5000,
            "rapid_feed": 20000,
            "lathe": {"turret_stations": 12, "chuck_diameter": 160, "chuck_length": 50,
                      "axes": {"c": true, "live_tools": true}}
        },
        "tools": {"tools": [
            {"number": 1, "name": "Turning", "shape": {"kind": "turning", "nose_radius": 0.4},
             "diameter": 20, "flute_length": 3, "length": 30}
        ]}
    })
}

/// The block on the mill-turn lathe, the setup's lathe values `lathe`:
/// a roughing of the outside and a hole 6 deep at 10 mm from the axis on
/// the face.
fn block(lathe: Value) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": mill_turn()}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_tool_put", "machine": "mill-turn", "tool": {
            "number": 5, "shape": {"kind": "drill"}, "diameter": 5, "mount": "axial",
            "cutting": {"spindle_rpm": 2000, "feed": 200, "plunge_feed": 100}}}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {
            "stock": {"kind": "around_bodies", "side": 2, "top": 1, "bottom": 5},
            "post": {"canned_cycles": true},
            "lathe": lathe
        }}),
    );
    for operation in [
        json!({"kind": {"type": "turn_rough", "side": "outside"}, "tool": 1, "stock_to_leave": 0.2}),
        json!({"kind": {"type": "drill"}, "tool": 5, "live": "face",
               "inputs": [{"kind": "point", "at": {"point": [30, 15, 10]}}],
               "depths": {"bottom": -6}}),
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
    doc
}

/// The turned part as the setup's work has it.
fn part(doc: &Document<MockKernel>) -> Value {
    let setups = query(doc, json!({"query": "cam_setups"}));
    setups["setups"][0]["work"]["turning"]["part"].clone()
}

/// The X (radius) of every point an operation's feed moves go to.
fn feeds(doc: &Document<MockKernel>, index: usize) -> Vec<[f64; 2]> {
    let paths = query(doc, json!({"query": "cam_toolpaths", "setup": 1}));
    paths["operations"][index]["turn_path"]["moves"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["kind"] == "line")
        .map(|m| [m["to"][0].as_f64().unwrap(), m["to"][1].as_f64().unwrap()])
        .collect()
}

fn widest(part: &Value) -> f64 {
    part["outside"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["x"].as_f64().unwrap())
        .fold(0.0, f64::max)
}

#[test]
fn the_spun_target_keeps_the_corners_off_the_section() {
    let corner = 20.0_f64.hypot(15.0);
    // The section's profile: 20 from the axis, the stock 2 over it; the
    // roughing goes down to it and its stock to leave, through the corners.
    let plain = block(json!({}));
    let section = part(&plain);
    assert_eq!(widest(&section), 20.0);
    assert_eq!(section["stock_radius"], json!(22.0));
    let lowest = feeds(&plain, 0)
        .iter()
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min);
    assert!((lowest - 20.2).abs() < 1e-6, "{lowest}");

    // Spun: the corners 25 from the axis (and the mesh's deviation), the
    // stock 2 over them; the roughing stays outside them.
    let spun = block(json!({"spun_target": true}));
    let turned = part(&spun);
    let reach = widest(&turned);
    assert!(reach >= corner && reach < corner + 0.01, "{turned}");
    let stock = turned["stock_radius"].as_f64().unwrap();
    assert!((stock - (reach + 2.0)).abs() < 1e-9, "{turned}");
    for p in feeds(&spun, 0) {
        assert!(p[0] >= corner, "the roughing cuts a corner at {p:?}");
    }
    // The setup keeps the choice; it is left out when off.
    let setups = query(&spun, json!({"query": "cam_setups"}));
    assert_eq!(setups["setups"][0]["lathe"]["spun_target"], json!(true));
    let setups = query(&plain, json!({"query": "cam_setups"}));
    assert!(setups["setups"][0]["lathe"].get("spun_target").is_none());

    // The live tool drills the block as it is: the same hole either way.
    for doc in [&plain, &spun] {
        let post = query(doc, json!({"query": "cam_post", "setup": 1}));
        let text = post["text"].as_str().unwrap();
        assert!(text.contains("G83 X20. C0. Z-6. "), "{text}");
    }
}

#[test]
fn a_limited_finish_cuts_only_its_stretch() {
    let mut doc = block(json!({}));
    let finish = command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1, "operation": {
            "kind": {"type": "turn_finish", "side": "outside"}, "tool": 1,
            "turning": {"limit_from": -3, "limit_to": -7}}}),
    );
    let id = finish["operation"].clone();
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let ops = generated["setups"][0]["operations"].as_array().unwrap();
    assert_eq!(ops[2]["state"], json!("up_to_date"), "{}", ops[2]);
    // Below the stock's surface the nose (0.4) stays between the limits.
    let points = feeds(&doc, 2);
    assert!(
        points.iter().any(|p| (p[0] - 20.0).abs() < 1e-6),
        "{points:?}"
    );
    for p in &points {
        if p[0] < 22.0 - 1e-6 {
            assert!(p[1] + 0.8 <= -3.0 + 1e-6, "past Z -3 at {p:?}");
            assert!(p[1] >= -7.0 - 1e-6, "past Z -7 at {p:?}");
        }
    }
    // The wrong way round: refused in words.
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": id, "changes": {
            "turning": {"limit_from": -7, "limit_to": -3}}}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let refused = &generated["setups"][0]["operations"][2];
    assert_eq!(refused["state"], json!("failed"), "{refused}");
    assert_eq!(
        refused["error"],
        json!("The limit towards the face must be in front of the limit towards the chuck.")
    );
}
