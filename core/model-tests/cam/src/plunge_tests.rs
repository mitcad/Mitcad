// SPDX-License-Identifier: MIT
//! Turn Finishing's Plunge through the JSON api (mitcad#2503, GitHub
//! mitcad/Mitcad#707) on the mock kernel's turned block: Don't plunge is
//! the default and is not written, so files and toolpaths stay as they
//! were; Tool geometry needs the tool's insert and holder and says so in
//! words until it has them; Free needs nothing. The paths of each mode on
//! a neck and in a bore are the engine's tests
//! (`core/cam/src/turning/plunge_tests.rs`).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The region of a sketch's first rectangle.
const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

const NEEDS_INSERT: &str = "Plunge by Tool geometry needs the insert and holder of tool 1: give them in Machines and Tools (Edit Tool), or choose Free or Don't plunge.";

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

/// A block 40 x 30 x 10 (the mock cuts it as a box) on a two-axis lathe
/// with a turning tool, turned about the design's Z: faced, roughed, and
/// finished once in each plunge mode.
fn finished_three_ways() -> Document<MockKernel> {
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
        json!({"cmd": "cam_machine_put", "machine": {
            "id": "lathe",
            "machine": {
                "name": "Lathe",
                "controller": "fanuc_turning",
                "travel": {"min": [-5, 0, -300], "max": [150, 0, 200]},
                "spindle": {"min_rpm": 50, "max_rpm": 3000, "nose_diameter": 0, "nose_length": 0},
                "max_feed": 5000,
                "rapid_feed": 20000,
                "lathe": {"turret_stations": 8, "chuck_diameter": 160, "chuck_length": 50}
            },
            "tools": {"tools": [
                {"number": 1, "name": "Turning", "shape": {"kind": "turning", "nose_radius": 0.4},
                 "diameter": 20, "flute_length": 3, "length": 30}
            ]}
        }}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {
            "stock": {"kind": "around_bodies", "side": 2, "top": 1, "bottom": 5},
            "post": {"canned_cycles": true},
            "lathe": {"program_number": 2001}
        }}),
    );
    for operation in [
        json!({"kind": {"type": "turn_face"}, "tool": 1}),
        json!({"kind": {"type": "turn_rough", "side": "outside"}, "tool": 1}),
        json!({"kind": {"type": "turn_finish", "side": "outside"}, "tool": 1}),
        json!({"kind": {"type": "turn_finish", "side": "outside", "plunge": "tool"}, "tool": 1}),
        json!({"kind": {"type": "turn_finish", "side": "outside", "plunge": "free"}, "tool": 1}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    doc
}

fn operations(doc: &Document<MockKernel>) -> Vec<Value> {
    let setups = query(doc, json!({"query": "cam_setups"}));
    setups["setups"][0]["operations"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn turn_finishing_plunges_as_chosen() {
    let mut doc = finished_three_ways();
    // Don't plunge is not written: the operation is as before.
    let ops = operations(&doc);
    assert_eq!(
        ops[2]["kind"],
        json!({"type": "turn_finish", "side": "outside"})
    );
    assert_eq!(ops[3]["kind"]["plunge"], json!("tool"));
    assert_eq!(ops[4]["kind"]["plunge"], json!("free"));

    // Tool geometry without the tool's insert and holder says so; Free on
    // a part without an undercut is the bridged finish.
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let states = &generated["setups"][0]["operations"];
    for i in [0, 1, 2, 4] {
        assert_eq!(states[i]["state"], json!("up_to_date"), "{}", states[i]);
    }
    assert_eq!(states[3]["state"], json!("failed"), "{}", states[3]);
    assert_eq!(states[3]["error"], json!(NEEDS_INSERT));
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let moves = |i: usize| {
        paths["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["id"] == ops[i]["id"])
            .map(|o| o["toolpath"]["moves"].clone())
            .unwrap_or_else(|| panic!("no toolpath of {}", ops[i]))
    };
    assert!(!moves(2).as_array().unwrap().is_empty());
    assert_eq!(moves(4), moves(2));

    // Back to Don't plunge: the kind is written without it again.
    let id = ops[4]["id"].clone();
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": id, "changes": {
            "kind": {"type": "turn_finish", "side": "outside"}}}),
    );
    assert_eq!(
        operations(&doc)[4]["kind"],
        json!({"type": "turn_finish", "side": "outside"})
    );

    // With its insert and holder (mitcad#2502: a VNMG in an SVJBR, a back
    // angle of 52 degrees) Tool geometry finishes too
    // (`CamTool::back_angle`).
    command(
        &mut doc,
        json!({"cmd": "cam_tool_put", "machine": "lathe", "tool": {
            "number": 1, "name": "Turning", "shape": {"kind": "turning", "nose_radius": 0.4},
            "diameter": 20, "flute_length": 3, "length": 30,
            "insert": {"insert": "VNMG 160404", "holder": "SVJBR 2525 M16"}}}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    for op in generated["setups"][0]["operations"].as_array().unwrap() {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }
    // On the block without an undercut it is the bridged finish.
    let paths = query(&doc, json!({"query": "cam_toolpaths", "setup": 1}));
    let moves = |i: usize| {
        paths["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["id"] == ops[i]["id"])
            .map(|o| o["toolpath"]["moves"].clone())
            .unwrap_or_else(|| panic!("no toolpath of {}", ops[i]))
    };
    assert_eq!(moves(3), moves(2));
}
