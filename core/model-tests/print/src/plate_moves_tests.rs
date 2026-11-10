// SPDX-License-Identifier: MIT
//! The plate an item left (mitcad#1595), with the mock kernel: an item is
//! on one plate at most, so `place_on_plate` moves one that is on another
//! plate, and its result lists each moved item with the plate it left
//! (`moved`) and says so in its `warnings`; a first placement, a change on
//! the same plate and a call that changes nothing list none. The undo text
//! names the move, undo and redo bring the item back and forth with its
//! count, and `mitcad-cli run`'s notes (`script_notes`) carry the move.

use serde_json::{Value, json};

use crate::print_files_tests::{command, design, small_block};
use mitcad_model::Document;
use mitcad_model::ids::BodyUid;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::{DocumentApi, ScriptApi, script_notes};

/// A query's answer.
fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// The items of each plate, in order, as `print_plates` lists them.
fn plate_items(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "print_plates"}))["plates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|plate| plate["items"].clone())
        .collect()
}

/// The fields of a `place_on_plate` result that the moves own, `moved`
/// and `warnings`, without the command envelope's own (`error`,
/// `recomputed`). The envelope's `notices` repeat the warnings, a warning
/// each (none without a move).
fn moves_of(result: &Value) -> Value {
    let notices: Vec<Value> = result["notices"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|notice| json!({"level": notice["level"], "message": notice["message"]}))
        .collect();
    let warnings: Vec<Value> = result["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|warning| json!({"level": "warning", "message": warning}))
        .collect();
    assert_eq!(notices, warnings, "{result}");
    json!({"moved": result["moved"], "warnings": result["warnings"]})
}

/// Two blocks of the root, Body1 and Body2, on no plate yet, and the
/// plates `one` (plate1) and `two` (plate2).
fn two_boxes() -> (Document<MockKernel>, BodyUid, BodyUid) {
    let mut doc = Document::new(MockKernel::default());
    let first = small_block(&mut doc);
    let second = small_block(&mut doc);
    assert_eq!(doc.body_name(first), "Body1");
    assert_eq!(doc.body_name(second), "Body2");
    command(&mut doc, json!({"cmd": "add_print_plate", "name": "one"}));
    command(&mut doc, json!({"cmd": "add_print_plate", "name": "two"}));
    (doc, first, second)
}

/// The steps of the report: Body1 twice and Body2 on `one`, then Body1 on
/// `two` without a count.
#[test]
fn an_item_put_on_another_plate_names_the_plate_it_left() {
    let (mut doc, first, second) = two_boxes();
    // A first placement moves nothing.
    let placed = command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "one",
               "items": [{"body": "Body1", "count": 2}, {"body": "Body2"}]}),
    );
    assert_eq!(moves_of(&placed), json!({"moved": [], "warnings": []}));

    // Body1 leaves `one` and keeps its count of 2 on `two`.
    let moved = command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "two", "items": [{"body": "Body1"}]}),
    );
    assert_eq!(
        moves_of(&moved),
        json!({
            "moved": [{"body": first.to_string(), "name": "Body1", "count": 2,
                       "from": {"id": "plate1", "name": "one", "count": 2},
                       "to": {"id": "plate2", "name": "two"}}],
            "warnings": ["Body1 moved from one to two (count 2)"],
        })
    );
    assert_eq!(doc.undo_label(), Some("Move Body1 from one to two"));
    let body1 = json!({"body": first.to_string(), "name": "Body1", "count": 2});
    let body2 = json!({"body": second.to_string(), "name": "Body2", "count": 1});
    assert_eq!(
        plate_items(&doc),
        vec![json!([body2.clone()]), json!([body1.clone()])],
        "Body2 stays on one"
    );

    // Undo puts Body1 back on `one` with its count; redo moves it again.
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        plate_items(&doc),
        vec![json!([body1.clone(), body2.clone()]), json!([])]
    );
    assert_eq!(doc.redo_label(), Some("Move Body1 from one to two"));
    command(&mut doc, json!({"cmd": "redo"}));
    assert_eq!(
        plate_items(&doc),
        vec![json!([body2.clone()]), json!([body1])]
    );

    // Back on `one` with a count of its own: the count given replaces the
    // one it had, and the result says what it was.
    let back = command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "plate1",
               "items": [{"body": first.to_string(), "count": 3}]}),
    );
    assert_eq!(
        back["moved"],
        json!([{"body": first.to_string(), "name": "Body1", "count": 3,
                "from": {"id": "plate2", "name": "two", "count": 2},
                "to": {"id": "plate1", "name": "one"}}])
    );
    assert_eq!(
        back["warnings"],
        json!(["Body1 moved from two to one (count 3, was 2)"])
    );
    assert_eq!(doc.undo_label(), Some("Move 3 of Body1 from two to one"));
    assert_eq!(
        plate_items(&doc),
        vec![
            json!([body2, {"body": first.to_string(), "name": "Body1", "count": 3}]),
            json!([])
        ]
    );
}

/// Occurrences move as bodies do; each moved item of a batch names its
/// own plate, in the order given, and an item placed for the first time
/// beside them is not listed.
#[test]
fn each_moved_item_of_a_batch_names_its_own_plate() {
    let mut doc = design();
    let bodies: Vec<BodyUid> = doc.bodies().iter().map(|b| b.uid).collect();
    let (root_body, arm_body) = (bodies[0], bodies[1]);
    let path = doc
        .instances()
        .into_iter()
        .find(|i| i.body == arm_body)
        .expect("the arm's body is placed")
        .path;
    let arm = path[0].to_string();
    let arm_name = doc.assembly().path_name(&path);
    let root_name = doc.body_name(root_body);
    for name in ["A", "B", "C"] {
        command(&mut doc, json!({"cmd": "add_print_plate", "name": name}));
    }
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "A",
               "items": [{"body": root_body.to_string(), "count": 3}, {"occurrence": arm}]}),
    );

    // Both leave A: the body with the count given, the occurrence with its
    // own.
    let moved = command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "B",
               "items": [{"body": root_body.to_string(), "count": 5},
                         {"occurrence": arm_name.clone()}]}),
    );
    assert_eq!(
        moves_of(&moved),
        json!({
            "moved": [
                {"body": root_body.to_string(), "name": root_name, "count": 5,
                 "from": {"id": "plate1", "name": "A", "count": 3},
                 "to": {"id": "plate2", "name": "B"}},
                {"occurrence": arm, "name": arm_name, "count": 1,
                 "from": {"id": "plate1", "name": "A", "count": 1},
                 "to": {"id": "plate2", "name": "B"}},
            ],
            "warnings": [
                format!("{root_name} moved from A to B (count 5, was 3)"),
                format!("{arm_name} moved from A to B (count 1)"),
            ],
        })
    );
    assert_eq!(doc.undo_label(), Some("Put 2 items on B"));

    // The arm's body on a plate for the first time, beside its occurrence
    // leaving B: only the occurrence moved.
    let moved = command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "C",
               "items": [{"body": arm_body.to_string()}, {"occurrence": arm, "count": 2}]}),
    );
    assert_eq!(
        moved["moved"],
        json!([{"occurrence": arm, "name": arm_name, "count": 2,
                "from": {"id": "plate2", "name": "B", "count": 1},
                "to": {"id": "plate3", "name": "C"}}])
    );
    assert_eq!(
        moved["warnings"],
        json!([format!("{arm_name} moved from B to C (count 2, was 1)")])
    );
    let items = plate_items(&doc);
    assert_eq!(items[0], json!([]));
    assert_eq!(items[1].as_array().unwrap().len(), 1, "the root's body");
    assert_eq!(
        items[2].as_array().unwrap().len(),
        2,
        "the arm's body and occurrence"
    );

    // Undone, the occurrence is back on B with its count; the arm's body
    // is on no plate.
    command(&mut doc, json!({"cmd": "undo"}));
    let items = plate_items(&doc);
    assert_eq!(items[1][1]["occurrence"], json!(arm));
    assert_eq!(items[1][1]["count"], 1);
    assert_eq!(items[2], json!([]));
}

/// A change on the plate an item is on, or a call that changes nothing,
/// moves nothing.
#[test]
fn an_item_on_its_own_plate_is_not_moved() {
    let (mut doc, _, _) = two_boxes();
    command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "one",
               "items": [{"body": "Body1", "count": 2}]}),
    );
    assert_eq!(doc.undo_label(), Some("Put 2 of Body1 on one"));

    // The same again: nothing changes, no undo step.
    let same = command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "one",
               "items": [{"body": "Body1", "count": 2}]}),
    );
    assert_eq!(moves_of(&same), json!({"moved": [], "warnings": []}));
    let unchanged = command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "one", "items": [{"body": "Body1"}]}),
    );
    assert_eq!(moves_of(&unchanged), json!({"moved": [], "warnings": []}));
    assert_eq!(doc.undo_label(), Some("Put 2 of Body1 on one"));

    // A new count on the same plate.
    let counted = command(
        &mut doc,
        json!({"cmd": "place_on_plate", "plate": "one",
               "items": [{"body": "Body1", "count": 4}, {"body": "Body2"}]}),
    );
    assert_eq!(moves_of(&counted), json!({"moved": [], "warnings": []}));
    assert_eq!(doc.undo_label(), Some("Put 2 items on one"));
    assert_eq!(plate_items(&doc)[0][0]["count"], 4);
}

/// `mitcad-cli run` prints a line for each moved item before its report.
#[test]
fn a_script_s_notes_carry_the_moves() {
    let (mut doc, _, _) = two_boxes();
    let results = doc
        .run_script(
            &json!([
                {"cmd": "place_on_plate", "plate": "one",
                 "items": [{"body": "Body1", "count": 2}, {"body": "Body2"}]},
                {"comment": "Body1 to two, then Body2 too, with three of it."},
                {"cmd": "place_on_plate", "plate": "two", "items": [{"body": "Body1"}]},
                {"cmd": "place_on_plate", "plate": "two",
                 "items": [{"body": "Body2", "count": 3}]},
            ])
            .to_string(),
        )
        .unwrap();
    assert_eq!(
        script_notes(&results),
        "Plates: Body1 moved from one to two (count 2)\n\
         Plates: Body2 moved from one to two (count 3, was 1)\n"
    );
    // A script without a move has none.
    let results = doc
        .run_script(
            &json!([{"cmd": "place_on_plate", "plate": "two",
                     "items": [{"body": "Body2", "count": 3}]}])
            .to_string(),
        )
        .unwrap();
    assert_eq!(script_notes(&results), "");
}
