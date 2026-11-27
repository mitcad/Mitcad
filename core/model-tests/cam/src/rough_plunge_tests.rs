// SPDX-License-Identifier: MIT
//! Turn Roughing's Plunge through the JSON api (mitcad#2549, GitHub
//! mitcad/Mitcad#733) on the mock kernel's turned block: Don't plunge is
//! the default and is not written, so older files keep their roughing; a
//! plunging roughing is written move by move while a roughing without a
//! plunge stays a G71; a finish into undercuts goes out of date when a
//! roughing of its side before it is edited, suppressed or moved, and
//! nothing else does. The paths into a neck, pockets split by a collar, a
//! bore's recess and a curved undercut are the engine's tests
//! (`core/cam/src/turning/rough_plunge_tests.rs`); which roughings let a
//! finish go deeper than its insert cuts is the model's
//! (`core/model/src/document/cam/turning_rough_tests.rs`).

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

/// A block 40 x 30 x 10 (the mock cuts it as a box) on a two-axis lathe
/// with a turning tool, turned about the design's Z, with cycles on:
/// faced, roughed without a plunge and with Free, finished without a
/// plunge and with Free.
fn roughed_both_ways() -> (Document<MockKernel>, Vec<Value>) {
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
    let mut ids = Vec::new();
    for operation in [
        json!({"kind": {"type": "turn_face"}, "tool": 1}),
        json!({"kind": {"type": "turn_rough", "side": "outside"}, "tool": 1, "stock_to_leave": 0.2}),
        json!({"kind": {"type": "turn_rough", "side": "outside", "plunge": "free"}, "tool": 1,
               "stock_to_leave": 0.2}),
        json!({"kind": {"type": "turn_finish", "side": "outside"}, "tool": 1}),
        json!({"kind": {"type": "turn_finish", "side": "outside", "plunge": "free"}, "tool": 1}),
    ] {
        let added = command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
        ids.push(added["operation"].clone());
    }
    (doc, ids)
}

fn operations(doc: &Document<MockKernel>) -> Vec<Value> {
    let setups = query(doc, json!({"query": "cam_setups"}));
    setups["setups"][0]["operations"]
        .as_array()
        .unwrap()
        .clone()
}

/// Each operation's state, in the order of `ids`.
fn states(doc: &Document<MockKernel>, ids: &[Value]) -> Vec<String> {
    let ops = operations(doc);
    ids.iter()
        .map(|id| {
            let op = ops
                .iter()
                .find(|o| o["id"] == *id)
                .unwrap_or_else(|| panic!("no operation {id}"));
            op["state"].as_str().unwrap_or_default().to_owned()
        })
        .collect()
}

fn generate_all(doc: &mut Document<MockKernel>) {
    let generated = command(doc, json!({"cmd": "cam_generate", "setup": 1}));
    for op in generated["setups"][0]["operations"].as_array().unwrap() {
        let state = &op["state"];
        assert!(state == "up_to_date" || state == "suppressed", "{op}");
    }
}

/// Don't plunge is not written on Turn Roughing; Free is, and back to
/// Don't plunge it is left out again.
#[test]
fn turn_roughing_writes_its_plunge_only_when_chosen() {
    let (mut doc, ids) = roughed_both_ways();
    let ops = operations(&doc);
    assert_eq!(
        ops[1]["kind"],
        json!({"type": "turn_rough", "side": "outside"})
    );
    assert_eq!(
        ops[2]["kind"],
        json!({"type": "turn_rough", "side": "outside", "plunge": "free"})
    );
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": ids[2], "changes": {
            "kind": {"type": "turn_rough", "side": "outside"}}}),
    );
    assert_eq!(
        operations(&doc)[2]["kind"],
        json!({"type": "turn_rough", "side": "outside"})
    );
    // Tool geometry without the tool's insert and holder fails in words,
    // as the finish's does.
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": ids[2], "changes": {
            "kind": {"type": "turn_rough", "side": "outside", "plunge": "tool"}}}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let rough = &generated["setups"][0]["operations"][2];
    assert_eq!(rough["state"], json!("failed"), "{rough}");
    assert_eq!(
        rough["error"],
        json!(
            "Plunge by Tool geometry needs the insert and holder of tool 1: give them in Machines and Tools (Edit Tool), or choose Free or Don't plunge."
        )
    );
}

/// In a program with both roughings, only the one without a plunge is a
/// G71; the plunging one is written move by move. The finishes stay a G70
/// over the G71's blocks (the block has no undercut: the same contour).
#[test]
fn only_the_roughing_without_a_plunge_is_a_g71() {
    let (mut doc, _) = roughed_both_ways();
    generate_all(&mut doc);
    let post = query(&doc, json!({"query": "cam_post", "setup": 1}));
    let text = post["text"].as_str().unwrap();
    assert_eq!(text.matches("G71 U").count(), 1, "{text}");
    assert_eq!(text.matches("G70 P").count(), 2, "{text}");
}

/// A finish into undercuts follows the roughings of its side before it:
/// editing the plunging roughing puts that roughing and the plunging
/// finish out of date and nothing else; so does suppressing the other
/// roughing (for the finish) and moving the finish before them.
#[test]
fn a_plunging_finish_follows_the_roughings_before_it() {
    let (mut doc, ids) = roughed_both_ways();
    generate_all(&mut doc);
    let ok = "up_to_date";
    let stale = "out_of_date";

    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": ids[2], "changes": {
            "kind": {"type": "turn_rough", "side": "outside"}}}),
    );
    assert_eq!(states(&doc, &ids), [ok, ok, stale, ok, stale]);
    generate_all(&mut doc);

    // The roughing's stock to leave.
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": ids[1], "changes": {
            "stock_to_leave": 0.3}}),
    );
    assert_eq!(states(&doc, &ids), [ok, stale, ok, ok, stale]);
    generate_all(&mut doc);

    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": ids[1], "changes": {
            "suppressed": true}}),
    );
    assert_eq!(states(&doc, &ids), [ok, "suppressed", ok, ok, stale]);
    generate_all(&mut doc);

    command(
        &mut doc,
        json!({"cmd": "cam_operation_move", "operation": ids[4], "index": 1}),
    );
    assert_eq!(states(&doc, &ids), [ok, "suppressed", ok, ok, stale]);
}
