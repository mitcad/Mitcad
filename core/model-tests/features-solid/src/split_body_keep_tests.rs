// SPDX-License-Identifier: MIT
//! Split Body keeping only some pieces (mitcad#2504) with the mock kernel,
//! its pieces measured by their bounds: a 30 x 20 x 10 box split by the yz
//! plane moved by a parameter. A side kept stays the kept side after the
//! plane moves; a point kept picks the piece that holds it; a piece to keep
//! that is gone warns, none left fails; the preview lists every piece with
//! a point inside it and what keeps it.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

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

/// A box F1 of 30 x 20 x 10 from the origin (body F1.b0) and the
/// parameter `cut` = 10, where the split's plane lies along x.
fn box_document() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let kernel = doc.kernel();
    kernel.box_faces.set(true);
    kernel.measures.set(true);
    // A plane square to an axis cuts the box's bounds; the bounds tell
    // inside from outside.
    kernel.split_bounds.set(true);
    kernel.copies_by_bounds.set(true);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 30, "width": 20, "height": 10, "operation": "new_body"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_parameter", "name": "cut", "expression": "10"}),
    );
    doc
}

fn split(keep: Value) -> Value {
    json!({"type": "split_body", "bodies": ["F1.b0"], "tool": "yz", "offset": "cut",
           "keep": keep})
}

/// The bodies' x extents: [min x, max x] of each, in the order the model
/// lists them.
fn extents(doc: &Document<MockKernel>) -> Vec<[f64; 2]> {
    query(doc, json!({"query": "bodies", "bbox": true}))
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            [
                b["bbox"]["min"][0].as_f64().unwrap(),
                b["bbox"]["max"][0].as_f64().unwrap(),
            ]
        })
        .collect()
}

/// A construction plane across the box's width, the xz plane moved to
/// y = 10 (a reference to an origin plane takes no offset of its own); its
/// uid, the further tool of a split.
fn across_plane(doc: &mut Document<MockKernel>) -> Value {
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xz", "distance": 10}}}),
    );
    added["uid"].clone()
}

fn feature(doc: &Document<MockKernel>, uid: &Value) -> Value {
    query(doc, json!({"query": "feature", "uid": uid}))
}

#[test]
fn a_kept_side_stays_kept_after_the_plane_moves() {
    let mut doc = box_document();
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": split(json!([{"side": "front"}]))}),
    );
    assert_eq!(feature(&doc, &added["uid"])["status"], "ok", "{added}");
    // One body: the piece in front of the plane, keeping the body's id.
    let bodies = query(&doc, json!({"query": "bodies"}));
    assert_eq!(bodies.as_array().unwrap().len(), 1, "{bodies}");
    assert_eq!(bodies[0]["uid"], "F1.b0");
    assert_eq!(extents(&doc), vec![[10.0, 30.0]]);
    // The plane moves past where the kept piece's middle was: the front
    // is still kept.
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "cut", "value": 25}),
    );
    assert_eq!(feature(&doc, &added["uid"])["status"], "ok");
    assert_eq!(extents(&doc), vec![[25.0, 30.0]]);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "cut", "value": 2}),
    );
    assert_eq!(extents(&doc), vec![[2.0, 30.0]]);
}

#[test]
fn without_keep_both_pieces_stay() {
    let mut doc = box_document();
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "split_body", "bodies": ["F1.b0"],
               "tool": "yz", "offset": "cut"}}),
    );
    assert_eq!(extents(&doc), vec![[0.0, 10.0], [10.0, 30.0]]);
}

#[test]
fn a_kept_point_picks_the_piece_that_holds_it() {
    let mut doc = box_document();
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": split(json!([{"point": [5, 10, 5]}]))}),
    );
    assert_eq!(feature(&doc, &added["uid"])["status"], "ok", "{added}");
    assert_eq!(extents(&doc), vec![[0.0, 10.0]]);
    // Both points: both pieces.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": added["uid"],
               "def": split(json!([{"point": [5, 10, 5]}, {"point": [20, 10, 5]}]))}),
    );
    assert_eq!(extents(&doc), vec![[0.0, 10.0], [10.0, 30.0]]);
}

#[test]
fn a_gone_piece_warns_and_none_left_fails() {
    let mut doc = box_document();
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature",
               "def": split(json!([{"point": [5, 10, 5]}, {"point": [100, 10, 5]}]))}),
    );
    // Built, with a warning.
    let state = feature(&doc, &added["uid"]);
    assert_eq!(state["status"], "warning", "{state}");
    let warnings = state["warnings"].to_string();
    assert!(
        warnings.contains("a piece to keep is gone (no piece holds point 2 (100, 10, 5))"),
        "{state}"
    );
    assert_eq!(extents(&doc), vec![[0.0, 10.0]]);
    // No piece holds any point: the split says so and fails.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": added["uid"],
               "def": split(json!([{"point": [100, 10, 5]}]))}),
    );
    let state = feature(&doc, &added["uid"]);
    assert_eq!(state["status"], "error", "{state}");
    assert!(
        state["error"]
            .to_string()
            .contains("none of the pieces to keep is left"),
        "{state}"
    );
}

#[test]
fn the_preview_lists_every_piece_with_what_keeps_it() {
    let mut doc = box_document();
    let preview = doc
        .preview(
            &json!({"cmd": "add_feature", "def": split(json!([{"side": "back"}]))}).to_string(),
        )
        .unwrap();
    let preview: Value = serde_json::from_str(&preview).unwrap();
    assert_eq!(preview["status"], "ok", "{preview}");
    assert_eq!(
        preview["piece_choices"],
        json!([
            {"body": "F1.b0", "point": [5.0, 10.0, 5.0], "keep": {"side": "back"}, "kept": true},
            {"body": "F1.b0", "point": [20.0, 10.0, 5.0], "keep": {"side": "front"}, "kept": false},
        ]),
        "{preview}"
    );
    // With further tools there is no single plane: the pieces are kept by
    // their points, four of them, every one kept without `keep`.
    let across = across_plane(&mut doc);
    let preview = doc
        .preview(
            &json!({"cmd": "add_feature", "def": {"type": "split_body", "bodies": ["F1.b0"],
                    "tool": "yz", "offset": "cut", "more_tools": [across]}})
            .to_string(),
        )
        .unwrap();
    let preview: Value = serde_json::from_str(&preview).unwrap();
    let choices = preview["piece_choices"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(choices.len(), 4, "{preview}");
    assert!(
        choices
            .iter()
            .all(|c| c["keep"].get("point").is_some() && c["kept"] == true),
        "{preview}"
    );
}

#[test]
fn a_side_needs_a_single_plane_tool() {
    let mut doc = box_document();
    let across = across_plane(&mut doc);
    let reply = doc.command(
        &json!({"cmd": "add_feature", "def": {"type": "split_body", "bodies": ["F1.b0"],
                "tool": "yz", "more_tools": [across], "keep": [{"side": "front"}]}})
        .to_string(),
    );
    // Refused, or added failing with the reason.
    let text = match reply {
        Ok(reply) => {
            let reply: Value = serde_json::from_str(&reply).unwrap();
            feature(&doc, &reply["uid"]).to_string()
        }
        Err(e) => e.to_string(),
    };
    assert!(
        text.contains("a side of the plane keeps pieces only"),
        "{text}"
    );
}
