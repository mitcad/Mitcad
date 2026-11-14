// SPDX-License-Identifier: MIT
//! The stock simulation through the JSON api (mitcad#1013, phase 3,
//! `cam_simulate`) on the mock kernel: a 40 x 30 x 10 block under 1 mm of
//! stock faced down to its top leaves nothing over it; snapshots carry the
//! changes that rebuild the stock; a pocket and a hole the block does not
//! have are gouges; the summary alone; the grid follows the smallest tool
//! and its cap.

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

/// A 40 x 30 block 10 high (F2.b0), a 12 x 10 rectangle at (10, 10) on a
/// second sketch (F3), a router with a 6 mm flat end mill (1) and a 5 mm
/// drill (2), and a setup with the stock 1 mm above the block (the work
/// zero at its top's front left corner) and its facing.
fn faced() -> Document<MockKernel> {
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
                {"number": 2, "shape": {"kind": "drill", "point_angle": 118}, "diameter": 5,
                 "flute_length": 15, "length": 25,
                 "cutting": {"spindle_rpm": 10000, "feed": 400, "plunge_feed": 200}}
            ]}
        }}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {"stock": {"kind": "around_bodies", "top": 1}}}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1,
               "operation": {"kind": {"type": "facing"}, "tool": 1}}),
    );
    doc
}

/// Applies runs (a full set, or the changes of a snapshot) to `z`.
fn apply(z: &mut [f64], runs: &Value) {
    let runs = runs.as_array().unwrap();
    assert_eq!(runs.len() % 2, 0);
    let mut k = 0usize;
    for pair in runs.chunks(2) {
        let (v, n) = (pair[0].as_i64().unwrap(), pair[1].as_i64().unwrap());
        let count = n.unsigned_abs() as usize;
        if n > 0 {
            for node in &mut z[k..k + count] {
                *node = v as f64 / 1000.0;
            }
        }
        k += count;
    }
    assert_eq!(k, z.len());
}

#[test]
fn a_faced_stock_leaves_nothing_over_the_block() {
    let doc = faced();
    let sim = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let grid = &sim["grid"];
    let (nx, ny) = (grid["nx"].as_u64().unwrap(), grid["ny"].as_u64().unwrap());
    // An eighth of the 6 mm end mill at most, over the 40 x 30 stock.
    assert!(number(&grid["step"][0]) <= 0.75 + 1e-9, "{grid}");
    assert!(nx >= 54 && ny >= 41, "{grid}");
    assert!(number(&grid["top"]).abs() < 1e-9, "{grid}");
    assert!((number(&grid["bottom"]) + 11.0).abs() < 1e-9, "{grid}");
    assert_eq!(sim["rapid_cuts"], json!([]), "{}", sim["rapid_cuts"]);
    assert!(number(&sim["removed"]) > 0.0);
    // Down to the block's top, 1 mm under the stock's.
    assert!(
        (number(&sim["lowest"]) + 1.0).abs() < 1e-9,
        "{}",
        sim["lowest"]
    );
    let left = &sim["remaining"];
    assert!(number(&left["max"]) <= 0.01, "{left}");
    assert!(number(&left["mean"]) <= 0.01, "{left}");
    assert_eq!(left["thickest"], json!([]), "{left}");
    assert_eq!(left["gouge"], json!(0.0), "{left}");
    assert_eq!(left["beside_nodes"], json!(0), "{left}");
    // The heights: all at the block's top.
    let mut z = vec![f64::NAN; (nx * ny) as usize];
    apply(&mut z, &sim["heights"]);
    assert!(z.iter().all(|h| (h + 1.0).abs() < 1e-9));
    apply(&mut z, &sim["part"]);
    assert!(z.iter().all(|h| (h + 1.0).abs() < 1e-9));
}

#[test]
fn snapshots_carry_the_changes_that_rebuild_the_stock() {
    let doc = faced();
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let id = paths["operations"][0]["id"].as_u64().unwrap();
    let moves = paths["operations"][0]["toolpath"]["moves"]
        .as_array()
        .unwrap()
        .len();
    assert!(moves > 4);
    let half = moves / 2;
    let sim = query(
        &doc,
        json!({"query": "cam_simulate", "setup": 1,
               "frames": [{"operation": id, "move": half}, {"operation": id, "move": 1},
                          {"operation": id, "move": moves + 10}]}),
    );
    let frames = sim["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 3, "{}", sim["frames"]);
    // In the program's order, a move past the last taken as the last.
    assert_eq!(frames[0]["move"], json!(1));
    assert_eq!(frames[1]["move"], json!(half));
    assert_eq!(frames[2]["move"], json!(moves - 1));
    let nodes =
        (sim["grid"]["nx"].as_u64().unwrap() * sim["grid"]["ny"].as_u64().unwrap()) as usize;
    let mut z = vec![f64::NAN; nodes];
    apply(&mut z, &sim["initial"]);
    for frame in frames {
        apply(&mut z, &frame["runs"]);
    }
    let mut end = vec![f64::NAN; nodes];
    apply(&mut end, &sim["heights"]);
    assert_eq!(z, end);

    // Up to the middle: something cut, not all.
    let partway = query(
        &doc,
        json!({"query": "cam_simulate", "setup": 1, "until": {"operation": id, "move": half}}),
    );
    assert_eq!(partway["until"], json!({"operation": id, "move": half}));
    let mut mid = vec![f64::NAN; nodes];
    apply(&mut mid, &partway["heights"]);
    assert!(mid.iter().any(|h| (h + 1.0).abs() < 1e-9));
    assert!(mid.iter().any(|h| h.abs() < 1e-9));
    assert!(number(&partway["remaining"]["max"]) > 0.99);
}

#[test]
fn a_pocket_and_a_hole_the_block_lacks_are_gouges() {
    let mut doc = faced();
    for operation in [
        json!({"kind": {"type": "pocket"}, "tool": 1,
               "inputs": [{"kind": "region", "sketch": "F3", "region": R}],
               "depths": {"bottom": -3}}),
        json!({"kind": {"type": "drill"}, "tool": 2,
               "inputs": [{"kind": "point", "at": {"point": [30, 15, 10]}}]}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    let sim = query(
        &doc,
        json!({"query": "cam_simulate", "setup": 1, "result": "remaining"}),
    );
    assert!(sim.get("heights").is_none() && sim.get("part").is_none());
    assert_eq!(sim["rapid_cuts"], json!([]), "{}", sim["rapid_cuts"]);
    let left = &sim["remaining"];
    // The hole goes through the stock's bottom 10 mm under the block's top.
    assert!(number(&left["gouge"]) > 9.0, "{left}");
    let gouges = left["gouges"].as_array().unwrap();
    // The deepest at the hole, then the pocket's floor 2 mm into the block.
    let at = &gouges[0]["at"];
    assert!(
        (number(&at[0]) - 30.0).abs() <= 1.0 && (number(&at[1]) - 15.0).abs() <= 1.0,
        "{left}"
    );
    assert!(
        gouges.iter().any(|g| {
            let (x, y) = (number(&g["at"][0]), number(&g["at"][1]));
            (10.0..=22.0).contains(&x)
                && (10.0..=20.0).contains(&y)
                && (number(&g["thickness"]) - 2.0).abs() < 0.01
        }),
        "{left}"
    );
    // Its place in the model: the work zero is the stock's top front left
    // corner, 11 up.
    assert!((number(&gouges[0]["model"][2]) - number(&gouges[0]["at"][2]) - 11.0).abs() < 1e-9);
}

#[test]
fn the_grid_keeps_to_its_cap_and_the_resolution_asked() {
    let doc = faced();
    let capped = query(
        &doc,
        json!({"query": "cam_simulate", "setup": 1, "max_nodes": 500, "result": "remaining"}),
    );
    let grid = &capped["grid"];
    assert!(
        grid["nx"].as_u64().unwrap() * grid["ny"].as_u64().unwrap() <= 500,
        "{grid}"
    );
    let coarse = query(
        &doc,
        json!({"query": "cam_simulate", "setup": 1, "resolution": 2, "result": "remaining"}),
    );
    let grid = &coarse["grid"];
    assert_eq!(
        (grid["nx"].clone(), grid["ny"].clone()),
        (json!(21), json!(16)),
        "{grid}"
    );
    let unknown = doc
        .query(
            &json!({"query": "cam_simulate", "setup": 1, "until": {"operation": 99, "move": 0}})
                .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(unknown.contains("not in the setup's program"), "{unknown}");
}
