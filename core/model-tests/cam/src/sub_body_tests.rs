// SPDX-License-Identifier: MIT
//! The sub-spindle's body and moving zones, a control that mirrors only Z
//! on it, and the main spindle's operations after the sub-spindle's
//! through the JSON api (mitcad#2392) on the mock kernel, with the block
//! parted off into the sub-spindle of sub_live_tests.rs: an operation on
//! the main spindle after the sub-spindle's refused at Generate and named
//! in the setups' data; a zone moving with the sub-spindle in Generate's
//! findings and the simulation's hits; the sub-spindle's body kept with the
//! machine; the hole on the back written at the C turned round for a
//! control that mirrors only Z.

use serde_json::{Value, json};

use mitcad_model_api::DocumentApi;

use crate::sub_live_tests::{block, command, lathe, query};

const MAIN_AFTER_SUB: &str =
    "It is on the main spindle after the sub-spindle's operations: move it before them.";

/// The setup's operations from `cam_setups`.
fn operations(doc: &mitcad_model::Document<mitcad_model::testing::MockKernel>) -> Vec<Value> {
    let setups = query(doc, json!({"query": "cam_setups"}));
    setups["setups"][0]["operations"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn a_main_spindle_operation_after_the_sub_spindle_s_is_refused_at_generate() {
    let mut doc = block();
    // A facing on the main spindle added last, after the back's operations.
    let added = command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1,
               "operation": {"kind": {"type": "turn_face"}, "tool": 1}}),
    );
    let id = added["operation"].as_u64().unwrap();
    // The setups' data names its problem, for the operation list and the
    // panels; the others have none.
    for op in operations(&doc) {
        if op["id"].as_u64() == Some(id) {
            assert_eq!(op["problem"], json!(MAIN_AFTER_SUB), "{op}");
        } else {
            assert!(op.get("problem").is_none(), "{op}");
        }
    }
    // Generate refuses it in words and computes the others.
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    for op in generated["setups"][0]["operations"].as_array().unwrap() {
        if op["id"].as_u64() == Some(id) {
            assert_eq!(op["state"], json!("failed"), "{op}");
            assert_eq!(op["error"], json!(MAIN_AFTER_SUB), "{op}");
        } else {
            assert_eq!(op["state"], json!("up_to_date"), "{op}");
        }
    }
    // The program is refused with it too.
    let refused = doc
        .query(&json!({"query": "cam_post", "setup": 1}).to_string())
        .unwrap_err()
        .to_string();
    assert!(refused.contains(MAIN_AFTER_SUB), "{refused}");
    // Suppressed it is no problem.
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": id, "changes": {"suppressed": true}}),
    );
    assert!(
        operations(&doc)
            .iter()
            .all(|op| op.get("problem").is_none())
    );
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": id, "changes": {"suppressed": false}}),
    );
    // Moved before the parting off it is generated with the others.
    command(
        &mut doc,
        json!({"cmd": "cam_operation_move", "operation": id, "index": 1}),
    );
    assert!(
        operations(&doc)
            .iter()
            .all(|op| op.get("problem").is_none())
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    for op in generated["setups"][0]["operations"].as_array().unwrap() {
        assert_eq!(op["state"], json!("up_to_date"), "{op}");
    }
    query(&doc, json!({"query": "cam_post", "setup": 1}));
}

/// The lathe with a guard that moves with the sub-spindle in front of its
/// jaws' face, the sub-spindle's chuck 10 mm long and its housing, and a
/// control that mirrors only Z on it when `z_only`.
fn guarded_lathe(z_only: bool) -> Value {
    let mut machine = lathe([-400.0, 300.0]);
    let lathe = &mut machine["machine"]["lathe"];
    lathe["sub_spindle"]["chuck_length"] = json!(10);
    lathe["sub_spindle"]["housing_diameter"] = json!(180);
    lathe["sub_spindle"]["housing_length"] = json!(200);
    lathe["sub_spindle"]["zones"] = json!([
        {"name": "Sub guard", "bounds": {"min": [0, -200, -60], "max": [1000, 200, 0]}}
    ]);
    if z_only {
        lathe["mill_turn"]["sub_mirror_z_only"] = json!(true);
    }
    machine
}

#[test]
fn a_zone_moving_with_the_sub_spindle_is_found_in_words() {
    let mut doc = block();
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": guarded_lathe(false)}),
    );
    // The machine keeps the sub-spindle's body and zones.
    let machines = query(&doc, json!({"query": "cam_machines"}));
    let kept = machines["machines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == json!("mill-turn"))
        .expect("the lathe")["machine"]["lathe"]["sub_spindle"]
        .clone();
    assert_eq!(kept["chuck_length"], json!(10.0), "{kept}");
    assert_eq!(kept["housing_diameter"], json!(180.0), "{kept}");
    assert_eq!(kept["zones"][0]["name"], json!("Sub guard"), "{kept}");
    // The work zero 200 mm from the machine's zero: the sub-spindle waits
    // at the far end of its travel (machine Z 300) for the main spindle's
    // operations, and holds the part pulled back 40 mm for its own.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {"placement": [0, 0, -200]}}),
    );
    let generated = command(&mut doc, json!({"cmd": "cam_generate", "setup": 1}));
    let messages: Vec<String> = generated["setups"][0]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["message"].as_str().unwrap().to_owned())
        .collect();
    let guard = "Turn Facing 2: the tool runs into Sub guard, which moves with the sub-spindle.";
    assert!(messages.iter().any(|m| m == guard), "{messages:?}");
    assert!(
        !messages
            .iter()
            .any(|m| m.starts_with("Turn Facing 1:") && m.contains("Sub guard")),
        "{messages:?}"
    );
    // The simulation gives the same hits.
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let hits = simulated["sub_spindle_hits"].as_array().unwrap();
    assert!(
        hits.iter().any(|h| h["message"] == json!(guard)),
        "{hits:?}"
    );
    // Without the guard nothing runs into it.
    let mut plain = guarded_lathe(false);
    plain["machine"]["lathe"]["sub_spindle"]["zones"] = json!([]);
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": plain}),
    );
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let hits = simulated["sub_spindle_hits"].as_array().unwrap();
    assert!(
        !hits
            .iter()
            .any(|h| h["message"].as_str().unwrap().contains("Sub guard")),
        "{hits:?}"
    );
}

/// The C of the hole's block in the drilling on the back.
fn hole_c(text: &str) -> f64 {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == "(DRILL 1)")
        .unwrap_or_else(|| panic!("no drilling:\n{text}"));
    let line = lines[start..]
        .iter()
        .find(|l| l.starts_with("G0 X") && l.contains(" C"))
        .unwrap_or_else(|| panic!("no C in the drilling:\n{text}"));
    line.split_whitespace()
        .find_map(|w| w.strip_prefix('C'))
        .and_then(|c| c.parse::<f64>().ok())
        .unwrap_or_else(|| panic!("no C in {line}"))
}

#[test]
fn a_z_only_control_gets_the_back_s_c_turned_round() {
    let mut doc = block();
    // The hole 5 mm off the axis both ways, an eighth of a turn round.
    let setups = query(&doc, json!({"query": "cam_setups"}));
    let drill = setups["setups"][0]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == json!("Drill 1"))
        .expect("the drilling")["id"]
        .clone();
    command(
        &mut doc,
        json!({"cmd": "cam_operation_edit", "operation": drill, "changes": {
            "inputs": [{"kind": "point", "at": {"point": [25, 20, 0]}}]}}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": guarded_lathe(false)}),
    );
    let turned = query(&doc, json!({"query": "cam_post", "setup": 1}));
    let c = hole_c(turned["text"].as_str().unwrap());
    assert!(
        (c.abs() - 45.0).abs() < 1e-6 || (c.abs() - 135.0).abs() < 1e-6,
        "{c}"
    );
    command(
        &mut doc,
        json!({"cmd": "cam_machine_put", "machine": guarded_lathe(true)}),
    );
    let mirrored = query(&doc, json!({"query": "cam_post", "setup": 1}));
    let text = mirrored["text"].as_str().unwrap();
    assert!((hole_c(text) + c).abs() < 1e-6, "{text}");
    // The rest of the program is the same: the hand-over, the turning.
    let lines = |t: &str| -> Vec<String> {
        t.lines()
            .take_while(|l| *l != "(DRILL 1)")
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(lines(text), lines(turned["text"].as_str().unwrap()));
}
