// SPDX-License-Identifier: MIT
//! Weld symbols on drawings' views (mitcad#2350) on the mock kernel: a
//! `weld` annotation on an edge drawn by the drawing's standard (ISO 2553
//! with its dashed identification line, AWS A2.4 without), a fillet on the
//! arrow side when no welds are given, edited as a whole and moved, kept in
//! the design file, refused on another type and when its sizes cannot be,
//! and carried by the SVG and DXF exports.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    match doc.command(&command.to_string()) {
        Ok(result) => panic!("{command} was accepted: {result}"),
        Err(e) => e.to_string(),
    }
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn undo_label(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "document"}))["undo"].clone()
}

fn sheet(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone()
}

fn annotation(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}: {}", sheet(doc)["annotations"]))
}

/// The weld symbol's group on the sheet.
fn group(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == format!("A{id}").as_str())
        .cloned()
        .unwrap_or_else(|| panic!("no group of annotation {id}"))
}

fn texts(group: &Value) -> Vec<String> {
    group["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["text"].as_str().map(str::to_owned))
        .collect()
}

fn dashed(group: &Value) -> bool {
    group["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i.get("line").is_some() && i["pen"] == "hidden")
}

/// A 40 x 30 x 20 block with its front's right upright side listed (x 40,
/// z 0 to 20); a drawing with its front view at 1:1, the side at sheet
/// x 120 from y 110 to 130. Returns the side's name.
fn welded() -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    doc.kernel().lists_geometry.set(true);
    let body: BodyUid = "F2.b0".parse().unwrap();
    let side = doc
        .kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .find_map(|e| e.name)
        .expect("a named edge");
    doc.kernel().edge_curves.borrow_mut().insert(
        side.clone(),
        CurveGeometry::Line {
            start: [40.0, 0.0, 0.0],
            end: [40.0, 0.0, 20.0],
        },
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    (doc, side)
}

fn pick(edge: &str) -> Value {
    json!({"body": "F2.b0", "edge": edge})
}

#[test]
fn a_weld_symbol_on_an_edge_is_drawn_by_the_drawings_standard() {
    let (mut doc, side) = welded();
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "weld",
               "picks": [pick(&side)], "at": [140, 140],
               "weld": {"arrow_side": {"symbol": "fillet", "size": 5},
                        "all_around": true, "tail": "141"}}),
    );
    let id = added["annotation"].as_u64().unwrap();
    assert_eq!(
        undo_label(&doc),
        format!("Add weld symbol {id} to Drawing1")
    );
    let weld = annotation(&doc, id);
    assert_eq!(weld["type"], "weld");
    assert_eq!(weld["weld"]["arrow_side"]["symbol"], "fillet");
    assert_eq!(weld["label"], "fillet a5 (arrow side) / all around / 141");
    let drawn = group(&doc, id);
    assert_eq!(drawn["class"], "weld-symbol");
    let words = texts(&drawn);
    assert!(
        words.contains(&"a5".to_owned()) && words.contains(&"141".to_owned()),
        "{words:?}"
    );
    assert!(dashed(&drawn), "ISO 2553's identification line: {drawn}");
    // The reference line starts where it was placed.
    let starts_there = drawn["items"].as_array().unwrap().iter().any(|i| {
        i["pen"] == "thin"
            && i["line"][0][0]
                .as_f64()
                .is_some_and(|x| (x - 140.0).abs() < 1e-6)
            && i["line"][0][1]
                .as_f64()
                .is_some_and(|y| (y - 140.0).abs() < 1e-6)
            && i["line"][1][1]
                .as_f64()
                .is_some_and(|y| (y - 140.0).abs() < 1e-6)
    });
    assert!(starts_there, "{drawn}");
    // Under ASME, AWS A2.4: a leg written bare, no dashed line.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing", "drawing": "D1", "standard": "asme"}),
    );
    let drawn = group(&doc, id);
    assert!(!dashed(&drawn), "{drawn}");
    assert!(
        texts(&drawn).contains(&"5".to_owned()),
        "{:?}",
        texts(&drawn)
    );
}

#[test]
fn a_weld_symbol_is_edited_moved_kept_and_exported() {
    let (mut doc, side) = welded();
    // No welds given: a fillet on the arrow side.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "weld",
               "picks": [pick(&side)]}),
    );
    let id = added["annotation"].as_u64().unwrap();
    assert_eq!(annotation(&doc, id)["label"], "fillet (arrow side)");
    // Its welds changed as a whole: a V with its penetration, ground flush,
    // a back weld on the other side, welded on site.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": id,
               "weld": {"arrow_side": {"symbol": "v", "penetration": 8, "contour": "flush",
                                       "finish": "grind"},
                        "other_side": {"symbol": "back_weld"},
                        "field": true}}),
    );
    assert_eq!(
        undo_label(&doc),
        format!("Edit weld symbol {id} of Drawing1")
    );
    let weld = annotation(&doc, id);
    assert_eq!(weld["weld"]["other_side"]["symbol"], "back_weld");
    assert_eq!(
        weld["label"],
        "V butt s8 (arrow side) / back weld (other side) / field weld"
    );
    let words = texts(&group(&doc, id));
    assert!(
        words.contains(&"s8".to_owned()) && words.contains(&"G".to_owned()),
        "{words:?}"
    );
    // Moved by a drag: its reference line from where it was by the drag.
    let before = annotation(&doc, id)["text_offset"].clone();
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": id,
               "move": [5, -2]}),
    );
    let after = annotation(&doc, id)["text_offset"].clone();
    // Unmoved before: its default place, 12 mm right of its arrow.
    let x = |v: &Value| v[0].as_f64().unwrap_or(12.0);
    assert!(
        (x(&after) - x(&before) - 5.0).abs() < 1e-6,
        "{before} {after}"
    );
    assert_eq!(
        undo_label(&doc),
        format!("Move weld symbol {id} of Drawing1")
    );
    // Kept in the design file.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let kept = &file["drawings"][0]["sheets"][0]["views"][0]["annotations"][0];
    assert_eq!(kept["type"], "weld");
    assert_eq!(kept["weld"]["arrow_side"]["finish"], "grind");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    // The exports draw it.
    let dir = mitcad_model::testing::unique_temp_dir("welded");
    for name in ["welded.svg", "welded.dxf"] {
        let path = dir.join(name);
        command(
            &mut doc,
            json!({"cmd": "export_drawing", "drawing": "D1", "path": path.to_string_lossy()}),
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("s8"), "{name}: no weld size");
    }
    let pdf = dir.join("welded.pdf");
    command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": "D1", "path": pdf.to_string_lossy()}),
    );
    assert!(std::fs::metadata(&pdf).unwrap().len() > 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn weld_symbols_refuse_what_they_cannot_be() {
    let (mut doc, side) = welded();
    // Welds on a note.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "note",
               "text": "WELD", "picks": [pick(&side)],
               "weld": {"arrow_side": {"symbol": "fillet"}}}),
    );
    assert!(error.contains("only a weld symbol has welds"), "{error}");
    // A pitch not longer than each weld.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "weld",
               "picks": [pick(&side)],
               "weld": {"arrow_side": {"symbol": "fillet", "length": 50, "pitch": 50}}}),
    );
    assert!(error.contains("pitch"), "{error}");
    // No weld on either side.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "weld",
               "picks": [pick(&side)], "weld": {"tail": "141"}}),
    );
    assert!(error.contains("arrow side"), "{error}");
    // An unknown symbol.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "weld",
               "picks": [pick(&side)], "weld": {"arrow_side": {"symbol": "rivet"}}}),
    );
    assert!(
        error.contains("rivet") || error.contains("variant"),
        "{error}"
    );
    // A text note has no welds.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "text",
               "at": [30, 60], "text": "NOTES"}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 1,
               "weld": {"arrow_side": {"symbol": "fillet"}}}),
    );
    assert!(error.contains("text note"), "{error}");
}
