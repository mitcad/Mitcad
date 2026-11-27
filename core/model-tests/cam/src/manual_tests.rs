// SPDX-License-Identifier: MIT
//! Manual G-code in a lathe setup through the JSON api (mitcad#2506) on the
//! mock kernel: a bar pull written after Parting Off, last and with an
//! operation after it, saved with the design and read back, moved along
//! when its operation is deleted, refused after an operation the setup does
//! not have; the program without it as before, the simulation unchanged by
//! it.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

/// The region of a sketch's first rectangle.
const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

/// The bar pulled out of the opened chuck with the parting blade.
const BAR_PULL: &str = "G0 X0. Z-20.\nM11 (CHUCK OPEN)\n\nG98 G1 Z5. F500.\nM10 (CHUCK CLOSE)\n";

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

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was taken: {result}"),
        Err(e) => e.to_string(),
    }
}

/// A two-axis lathe on a Fanuc control with a turning tool and a parting
/// blade.
fn lathe() -> Value {
    json!({
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
             "diameter": 20, "flute_length": 3, "length": 30},
            {"number": 2, "name": "Parting", "shape": {"kind": "parting", "width": 3},
             "diameter": 20, "flute_length": 25, "length": 30}
        ]}
    })
}

/// A block 40 x 30 x 10 turned on the lathe: faced, roughed and parted
/// off.
fn bar_job() -> Document<MockKernel> {
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
        json!({"cmd": "cam_machine_put", "machine": lathe()}),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_setup_add", "setup": {
            "stock": {"kind": "around_bodies", "side": 2, "top": 1, "bottom": 5},
            "lathe": {"program_number": 2001}
        }}),
    );
    for operation in [
        json!({"kind": {"type": "turn_face"}, "tool": 1}),
        json!({"kind": {"type": "turn_rough", "side": "outside"}, "tool": 1, "stock_to_leave": 0.2}),
        json!({"kind": {"type": "turn_part_off"}, "tool": 2, "turning": {"peck": 3}}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "cam_operation_add", "setup": 1, "operation": operation}),
        );
    }
    doc
}

/// The setup's operations' ids by name.
fn operation(doc: &Document<MockKernel>, name: &str) -> u64 {
    let setups = query(doc, json!({"query": "cam_setups"}));
    setups["setups"][0]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == json!(name))
        .unwrap_or_else(|| panic!("no operation {name}"))["id"]
        .as_u64()
        .unwrap()
}

fn posted(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "cam_post", "setup": 1}))
}

fn text(post: &Value) -> String {
    post["text"].as_str().unwrap().to_owned()
}

/// Sets the setup's manual G-code (its lathe values kept).
fn set_manual(doc: &mut Document<MockKernel>, manual: Value) {
    command(
        doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {
            "lathe": {"program_number": 2001, "manual_code": manual}
        }}),
    );
}

#[test]
fn a_bar_pull_after_parting_off_is_written_saved_and_read_back() {
    let mut doc = bar_job();
    let before = text(&posted(&doc));
    let simulated = query(&doc, json!({"query": "cam_simulate", "setup": 1}));
    let part_off = operation(&doc, "Parting Off 1");
    set_manual(
        &mut doc,
        json!([{"name": "Bar pull", "after": part_off, "text": BAR_PULL}]),
    );

    // Parting Off last: the block after it, before the end; its lines in
    // order, blank ones left out; the modes, the work offset, the coolant
    // and the spindle stated again after it.
    let post = posted(&doc);
    let with = text(&post);
    let parting = with.find("(PARTING OFF 1)").expect("the parting off");
    let begin = with
        .find("(MANUAL CODE BEGIN: BAR PULL, NOT SIMULATED OR READ BACK)\n")
        .expect("the block");
    assert!(parting < begin, "{with}");
    assert!(
        with.contains(
            "M5\n(MANUAL CODE BEGIN: BAR PULL, NOT SIMULATED OR READ BACK)\n\
             G0 X0. Z-20.\nM11 (CHUCK OPEN)\nG98 G1 Z5. F500.\nM10 (CHUCK CLOSE)\n\
             (MANUAL CODE END)\nG21 G18 G40 G99\nG54\nM9\nM5\nG0 X"
        ),
        "{with}"
    );
    assert!(with.ends_with("M30\n%\n"), "{with}");
    // Without the block's part, the program as before.
    let end = with.rfind("M30\n").unwrap();
    assert_eq!(format!("{}{}", &with[..begin], &with[end..]), before);
    // The manual lines named for the panel: from the comment that opens
    // them to the one that closes them.
    let lines: Vec<&str> = with.lines().collect();
    let manual = post["manual"].as_array().expect("the manual lines");
    assert_eq!(manual.len(), 1);
    assert_eq!(manual[0]["name"], json!("Bar pull"));
    let from = manual[0]["from"].as_u64().unwrap() as usize;
    let to = manual[0]["to"].as_u64().unwrap() as usize;
    assert!(lines[from - 1].starts_with("(MANUAL CODE BEGIN: BAR PULL"));
    assert_eq!(lines[to - 1], "(MANUAL CODE END)");
    assert_eq!(to - from, 5);
    // Outside the simulation.
    assert_eq!(
        query(&doc, json!({"query": "cam_simulate", "setup": 1})),
        simulated
    );

    // Saved with the design and read back: the text and the place.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["cam"]["setups"][0]["lathe"]["manual_code"],
        json!([{"name": "Bar pull", "after": part_off, "text": BAR_PULL}])
    );
    let mut reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(reopened.to_json(), saved);
    reopened.recompute();
    assert_eq!(text(&posted(&reopened)), with);

    // A setup without the block writes the program as before.
    command(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {
            "lathe": {"program_number": 2001}
        }}),
    );
    let after = posted(&doc);
    assert_eq!(text(&after), before);
    assert!(after.get("manual").is_none(), "{after}");
}

#[test]
fn a_bar_pull_between_parting_off_and_the_next_operation() {
    let mut doc = bar_job();
    command(
        &mut doc,
        json!({"cmd": "cam_operation_add", "setup": 1,
               "operation": {"kind": {"type": "turn_face"}, "tool": 1}}),
    );
    let part_off = operation(&doc, "Parting Off 1");
    set_manual(
        &mut doc,
        json!([{"name": "Bar pull", "after": part_off, "text": BAR_PULL}]),
    );
    let with = text(&posted(&doc));
    // The next operation from its tool change: the turret indexed, the
    // spindle's limit and speed again.
    let begin = with.find("(MANUAL CODE BEGIN: BAR PULL").unwrap();
    let next = with.find("(TURN FACING 2)").unwrap();
    assert!(with.find("(PARTING OFF 1)").unwrap() < begin && begin < next);
    assert!(
        with.contains("(MANUAL CODE END)\nG21 G18 G40 G99\nG54\nM9\nM5\n(TURN FACING 2)\nG0 X"),
        "{with}"
    );
    let tail = &with[next..];
    let change = tail.find("\nT0101\nG50 S").expect("the tool change");
    assert!(tail[change..].contains("\nG96 S"), "{with}");
}

#[test]
fn the_block_keeps_its_place_and_follows_only_operations_of_its_setup() {
    let mut doc = bar_job();
    let rough = operation(&doc, "Turn Roughing 1");
    let part_off = operation(&doc, "Parting Off 1");
    // After an operation the setup does not have.
    let error = refused(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {
            "lathe": {"manual_code": [{"name": "Bar pull", "after": 999, "text": "M5"}]}
        }}),
    );
    assert!(
        error.contains("The manual G-code Bar pull follows an operation the setup does not have."),
        "{error}"
    );
    // Without code.
    let error = refused(
        &mut doc,
        json!({"cmd": "cam_setup_edit", "setup": 1, "changes": {
            "lathe": {"manual_code": [{"name": "Bar pull", "text": " \n "}]}
        }}),
    );
    assert!(
        error.contains("The manual G-code Bar pull has no code."),
        "{error}"
    );
    // What the control would not run, at Post Process, in words.
    set_manual(
        &mut doc,
        json!([{"name": "Bar pull", "after": part_off, "text": "M5\nM30"}]),
    );
    let error = doc
        .query(&json!({"query": "cam_post", "setup": 1}).to_string())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(
            "The manual G-code Bar pull cannot be written for Fanuc (turning): its line 2 has M30"
        ),
        "{error}"
    );
    // Its operation deleted: it follows the one before.
    set_manual(
        &mut doc,
        json!([{"name": "Bar pull", "after": part_off, "text": BAR_PULL}]),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_operation_delete", "operation": part_off}),
    );
    let setups = query(&doc, json!({"query": "cam_setups"}));
    assert_eq!(
        setups["setups"][0]["lathe"]["manual_code"][0]["after"],
        json!(rough)
    );
    let with = text(&posted(&doc));
    assert!(
        with.find("(TURN ROUGHING 1)").unwrap() < with.find("(MANUAL CODE BEGIN").unwrap(),
        "{with}"
    );
    // The first one deleted: before the first operation.
    let face = operation(&doc, "Turn Facing 1");
    set_manual(
        &mut doc,
        json!([{"name": "Bar pull", "after": face, "text": BAR_PULL}]),
    );
    command(
        &mut doc,
        json!({"cmd": "cam_operation_delete", "operation": face}),
    );
    let setups = query(&doc, json!({"query": "cam_setups"}));
    assert!(
        setups["setups"][0]["lathe"]["manual_code"][0]
            .get("after")
            .is_none(),
        "{setups}"
    );
    let with = text(&posted(&doc));
    assert!(
        with.find("(MANUAL CODE END)").unwrap() < with.find("(TURN ROUGHING 1)").unwrap(),
        "{with}"
    );
}
