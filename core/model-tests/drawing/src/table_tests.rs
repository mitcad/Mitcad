// SPDX-License-Identifier: MIT
//! Tables on drawings' views (mitcad#707 phase 3) on the mock kernel: a
//! hole table of a top view, its tags, origins, columns and sort, the
//! model changed under it; a parts list of an assembly's view with its
//! balloons, the design changed under them; undo, the file, the export
//! and refusals.

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
        .unwrap_or_else(|| panic!("no annotation {id}"))
}

fn texts(doc: &Document<MockKernel>) -> Vec<String> {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["items"].as_array().unwrap().clone())
        .filter_map(|i| i["text"].as_str().map(str::to_owned))
        .collect()
}

/// A 40 x 30 x 20 block at the origin (F2.b0) and a 20 x 10 x 10 block at
/// x 50 (F4.b0), each with a hole of 6.6 through it from the middle of its
/// top; a drawing with a front view at 1:1 and the top view above it.
fn holed() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_holes.set(Some(6.6));
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [50, 0], "width": 20, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
        ]),
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [100, 120], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "projected", "parent": 1,
               "side": "above"}),
    );
    doc
}

#[test]
fn a_hole_table_tags_and_lists_the_holes_its_view_sees() {
    let mut doc = holed();
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "hole_table",
               "at": [250, 270]}),
    );
    assert_eq!(added["annotation"], 1);
    assert_eq!(undo_label(&doc), "Add hole table 1 to Drawing1");
    // Both holes of one size: A1 and A2 along the view's x, from its lower
    // left corner.
    let table = annotation(&doc, 1);
    assert_eq!(table["broken"], false, "{table}");
    assert_eq!(table["rows"], 2);
    assert_eq!(
        table["label"], "A1 20 15 Ø6.6 THRU -; A2 60 5 Ø6.6 THRU -",
        "{table}"
    );
    assert_eq!(
        table["columns"],
        json!(["tag", "x", "y", "size", "depth", "thread"])
    );
    let all = texts(&doc);
    for text in ["HOLES (mm)", "TAG", "Ø6.6", "THRU"] {
        assert!(all.iter().any(|t| t == text), "{text}: {all:?}");
    }
    // Each tag twice: in the table and at its hole.
    assert_eq!(all.iter().filter(|t| *t == "A2").count(), 2, "{all:?}");
    // The front view sees them from the side: none.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "hole_table",
               "at": [30, 60]}),
    );
    assert_eq!(annotation(&doc, 2)["label"], "no holes");

    // Numbered, from the component's origin, three columns sorted by x
    // the other way.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 1,
               "table": {"tags": "numbers", "origin": "model", "columns": ["tag", "x", "y"],
                         "sort": "x", "descending": true, "title": "BORES"}}),
    );
    assert_eq!(undo_label(&doc), "Edit hole table 1 of Drawing1");
    let table = annotation(&doc, 1);
    assert_eq!(table["label"], "2 60 5; 1 20 15", "{table}");
    assert!(texts(&doc).iter().any(|t| t == "BORES"));
    // Its origin a point of the model: it follows the model.
    let body: BodyUid = "F2.b0".parse().unwrap();
    let edge = doc
        .kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .find_map(|e| e.name)
        .unwrap();
    doc.kernel().edge_curves.borrow_mut().insert(
        edge.clone(),
        CurveGeometry::Line {
            start: [10.0, 10.0, 20.0],
            end: [30.0, 10.0, 20.0],
        },
    );
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 1,
               "picks": [{"body": "F2.b0", "edge": edge, "point": "start"}],
               "table": {"sort": null, "descending": false}}),
    );
    assert_eq!(annotation(&doc, 1)["label"], "1 10 5; 2 50 -5");
    doc.kernel().edge_curves.borrow_mut().insert(
        edge.clone(),
        CurveGeometry::Line {
            start: [12.0, 10.0, 20.0],
            end: [30.0, 10.0, 20.0],
        },
    );
    assert_eq!(annotation(&doc, 1)["label"], "1 8 5; 2 48 -5");
    // Moved on the sheet.
    let before = annotation(&doc, 1)["box"].clone();
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 1,
               "move": [-20, 5]}),
    );
    assert_eq!(undo_label(&doc), "Move hole table 1 of Drawing1");
    let after = annotation(&doc, 1);
    assert_ne!(after["box"], before);
    assert_eq!(after["table"]["place"], json!([230.0, 275.0]));
    // The holes gone from the model: none listed. The holes are kept by
    // body version (mitcad#718), so the bodies change with them.
    doc.kernel().box_holes.set(None);
    for (uid, sketch, distance) in [("F2", "F1", 21), ("F4", "F3", 11)] {
        command(
            &mut doc,
            json!({"cmd": "edit_feature", "uid": uid, "def": {"type": "extrude",
                   "profiles": [{"sketch": sketch, "region": R}],
                   "extent": {"type": "distance", "distance": distance},
                   "operation": "new_body"}}),
        );
    }
    assert_eq!(annotation(&doc, 1)["label"], "no holes");
    doc.kernel().box_holes.set(Some(6.6));
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotation(&doc, 1)["label"], "1 8 5; 2 48 -5");

    // Saved and read again; undone.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let kept = &file["drawings"][0]["sheets"][0]["views"][1]["annotations"][0];
    assert_eq!(kept["type"], "hole_table");
    assert_eq!(kept["table"]["tags"], "numbers");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotation(&doc, 1)["table"]["place"], json!([250.0, 270.0]));
}

#[test]
fn tables_refuse_what_they_cannot_be() {
    let mut doc = holed();
    for (cmd, message) in [
        (
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2,
                   "type": "hole_table"}),
            "needs its place on the sheet",
        ),
        (
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2,
                   "type": "hole_table", "at": [250, 270], "table": {"columns": ["colour"]}}),
            "not colour",
        ),
        (
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2,
                   "type": "parts_list", "at": [250, 270], "table": {"tags": "numbers"}}),
            "no tags or origin",
        ),
        (
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "note",
                   "picks": [[0, 0]], "text": "X", "table": {"sort": "x"}}),
            "has no table settings",
        ),
    ] {
        let error = refused(&mut doc, cmd);
        assert!(error.contains(message), "{error}");
    }
    // A pictorial view sees no hole end on.
    let iso = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "iso",
               "at": [320, 200], "scale": "1:1"}),
    )["view"]
        .clone();
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": iso,
               "type": "hole_table", "at": [250, 270]}),
    );
    assert!(error.contains("from an angle"), "{error}");
}

/// An assembly: Plate (a 40 x 30 x 5 block, F2.b0) placed twice (the
/// second 50 along X), Pin (a 10 x 10 x 20 block, F4.b0) once; a drawing
/// of the design's top view. Returns the occurrences: Plate's two, Pin's.
fn assembly() -> (Document<MockKernel>, [String; 3]) {
    let mut doc = Document::new(MockKernel::default());
    let plate = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate"}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    let second = command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": plate,
               "transform": {"translation": [50, 0, 0]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    let pin = command(&mut doc, json!({"cmd": "create_component", "name": "Pin"}))["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 40], "width": 10, "height": 10},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [150, 150], "scale": "1:1"}),
    );
    (doc, [plate, second, pin])
}

fn edge_of(doc: &Document<MockKernel>, body: &str) -> String {
    let body: BodyUid = body.parse().unwrap();
    doc.kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .find_map(|e| e.name)
        .unwrap()
}

#[test]
fn a_parts_list_and_its_balloons_follow_the_design() {
    let (mut doc, [_, second, pin]) = assembly();
    let list = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "parts_list",
               "at": [300, 280]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(
        undo_label(&doc),
        format!("Add parts list {list} to Drawing1")
    );
    // Sorted by name and numbered: Pin, then Plate twice.
    let parts = annotation(&doc, list);
    assert_eq!(parts["label"], "1 Pin x1; 2 Plate x2", "{parts}");
    assert_eq!(
        parts["columns"],
        json!(["item", "quantity", "name", "description", "material"])
    );
    let all = texts(&doc);
    for text in ["BILL OF MATERIALS", "ITEM", "QTY", "Plate", "Pin"] {
        assert!(all.iter().any(|t| t == text), "{text}: {all:?}");
    }
    // Balloons on Plate's second placement and on Pin: their rows' numbers.
    let plate_edge = edge_of(&doc, "F2.b0");
    let pin_edge = edge_of(&doc, "F4.b0");
    let on_plate = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "balloon",
               "picks": [{"body": "F2.b0", "occurrence": second, "edge": plate_edge}],
               "at": [260, 200]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(
        undo_label(&doc),
        format!("Add balloon {on_plate} to Drawing1")
    );
    let on_pin = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "balloon",
               "picks": [{"body": "F4.b0", "occurrence": pin, "edge": pin_edge}]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(annotation(&doc, on_plate)["label"], "2");
    assert_eq!(annotation(&doc, on_pin)["label"], "1");
    assert!(
        sheet(&doc)["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["class"] == "balloon")
    );
    // Dragged: it keeps its place from what it picks.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": on_plate,
               "move": [5, 5]}),
    );
    assert_eq!(
        undo_label(&doc),
        format!("Move balloon {on_plate} of Drawing1")
    );
    // Sorted the other way: the numbers follow.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": list,
               "table": {"descending": true, "columns": ["item", "name", "quantity", "Vendor"]}}),
    );
    assert_eq!(annotation(&doc, list)["label"], "1 Plate x2; 2 Pin x1");
    assert_eq!(annotation(&doc, on_plate)["label"], "1");
    assert!(texts(&doc).iter().any(|t| t == "VENDOR"));

    // The second placement deleted: one Plate, its balloon broken.
    command(
        &mut doc,
        json!({"cmd": "delete_occurrence", "occurrence": second}),
    );
    assert_eq!(annotation(&doc, list)["label"], "1 Plate x1; 2 Pin x1");
    let broken = annotation(&doc, on_plate);
    assert_eq!(broken["broken"], true, "{broken}");
    assert!(
        broken["error"]
            .as_str()
            .unwrap()
            .contains("does not show the body F2.b0"),
        "{broken}"
    );
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotation(&doc, list)["label"], "1 Plate x2; 2 Pin x1");
    assert_eq!(annotation(&doc, on_plate)["broken"], false);

    // Without its parts list a balloon is broken too.
    command(
        &mut doc,
        json!({"cmd": "delete_drawing_annotation", "drawing": "D1", "annotation": list}),
    );
    let unlisted = annotation(&doc, on_pin);
    assert!(
        unlisted["error"]
            .as_str()
            .unwrap()
            .contains("no parts list on the sheet lists Pin"),
        "{unlisted}"
    );
    command(&mut doc, json!({"cmd": "undo"}));

    // The export draws them.
    let dir = mitcad_model::testing::unique_temp_dir("parts-list");
    let svg = dir.join("parts-list.svg");
    command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": "D1", "path": svg.to_string_lossy()}),
    );
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(
        text.contains("BILL OF MATERIALS") && text.contains("Plate"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_parts_list_places_its_balloons_when_asked() {
    // mitcad#725: with `balloons`, a parts list puts a balloon on each of
    // its rows' bodies the view shows, in its own undo step.
    let (mut doc, _) = assembly();
    // The bodies' edges, which the balloons are put on, as the kernel lists
    // them (the mock lists none unless asked).
    doc.kernel().lists_geometry.set(true);
    let depth = doc.undo_depth();
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "parts_list",
               "at": [300, 280], "balloons": true}),
    );
    assert_eq!(doc.undo_depth(), depth + 1);
    let list = added["annotation"].as_u64().unwrap();
    let balloons: Vec<u64> = added["balloons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_u64().unwrap())
        .collect();
    // One per row: Pin and Plate, whose two placements are one row.
    assert_eq!(balloons.len(), 2, "{added}");
    assert_eq!(
        undo_label(&doc),
        format!("Add parts list {list} and 2 balloons to Drawing1")
    );
    let mut labels: Vec<String> = balloons
        .iter()
        .map(|b| annotation(&doc, *b)["label"].as_str().unwrap().to_owned())
        .collect();
    labels.sort();
    assert_eq!(labels, ["1", "2"]);
    for b in &balloons {
        assert_eq!(annotation(&doc, *b)["type"], "balloon");
        assert_ne!(annotation(&doc, *b)["broken"], true);
    }
    // Undone at once, list and balloons.
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(sheet(&doc)["annotations"].as_array().unwrap().is_empty());
    // Without it, the list alone, as before; another type takes none.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "parts_list",
               "at": [300, 280]}),
    );
    assert!(added["balloons"].as_array().unwrap().is_empty(), "{added}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "hole_table",
               "at": [20, 280], "balloons": true}),
    );
    assert!(error.contains("places no balloons"), "{error}");
}

#[test]
fn picks_on_an_occurrences_view_name_the_occurrence() {
    // mitcad#717: a view of Plate's second placement only (50 along X):
    // its front top edge (y 0, z 5 in Plate) at y 0 of the view.
    let (mut doc, [_, second, _]) = assembly();
    doc.kernel().lists_geometry.set(true);
    let edge = edge_of(&doc, "F2.b0");
    doc.kernel().edge_curves.borrow_mut().insert(
        edge.clone(),
        CurveGeometry::Line {
            start: [0.0, 0.0, 5.0],
            end: [40.0, 0.0, 5.0],
        },
    );
    let view = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [300, 150], "scale": "1:1",
               "subject": {"type": "occurrence", "occurrence": second}}),
    )["view"]
        .clone();
    // Its box 50..90 x 0..30 of the design, its middle at 300, 150: the
    // edge at y 135 on the sheet.
    let picked = query(
        &doc,
        json!({"query": "drawing_pick", "drawing": "D1", "view": view, "at": [300, 135.2],
               "want": "line"}),
    );
    assert_eq!(picked["pick"]["edge"], edge.as_str(), "{picked}");
    assert_eq!(picked["pick"]["occurrence"], second.as_str(), "{picked}");
    assert_eq!(picked["pick"]["body"], "F2.b0");
}

#[test]
fn a_parts_list_of_a_subassemblys_view_lists_what_is_in_it() {
    // Sub (placed once in the root) holds Plate twice. A view of the
    // design lists Sub; a view of Sub's occurrence lists its Plates, and a
    // balloon on the second Plate takes its number there.
    let mut doc = Document::new(MockKernel::default());
    let sub = command(&mut doc, json!({"cmd": "create_component", "name": "Sub"}))["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    let plate = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Plate"}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
        ]),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "Sub"}),
    );
    let second = command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": plate,
               "transform": {"translation": [50, 0, 0]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [120, 150], "scale": "1:1"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [120, 230], "scale": "1:1",
               "subject": {"type": "occurrence", "occurrence": sub}}),
    );
    let design = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "parts_list",
               "at": [250, 120]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let inside = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "parts_list",
               "at": [250, 270]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(annotation(&doc, design)["label"], "1 Sub x1");
    assert_eq!(annotation(&doc, inside)["label"], "1 Plate x2");
    // Parts only: Plate twice in the design's view too.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": design,
               "table": {"levels": "parts"}}),
    );
    assert_eq!(annotation(&doc, design)["label"], "1 Plate x2");
    command(&mut doc, json!({"cmd": "undo"}));
    // A balloon on the second Plate in Sub's view: Plate's row there.
    let edge = edge_of(&doc, "F2.b0");
    let balloon = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "balloon",
               "picks": [{"body": "F2.b0", "occurrence": format!("{sub}/{second}"), "edge": edge}]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let shown = annotation(&doc, balloon);
    assert_eq!(shown["broken"], false, "{shown}");
    assert_eq!(shown["label"], "1");
}

#[test]
fn grouped_parts_list_keeps_every_components_balloon_number_and_material() {
    let (mut doc, [_, second, _]) = assembly();
    doc.kernel().measures.set(true);
    let copy = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "PlateCopy"}),
    );
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [60, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": R}],
                "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
            {"cmd": "activate_component", "component": "C0"}
        ]),
    );
    // The view keeps what it showed when placed (mitcad#1293): PlateCopy,
    // made after it, comes in by Update from Model, and its parts list
    // lists what the view shows.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "subject": {"type": "design"}}),
    );
    let list = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
            "type": "parts_list", "at": [300, 280], "table": {"group_identical": true}
        }),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(annotation(&doc, list)["label"], "1 Pin x1; 2 Plate x3");
    let mut balloons = Vec::new();
    for (body, occurrence) in [
        ("F2.b0", json!(second)),
        ("F6.b0", copy["occurrence"].clone()),
    ] {
        let edge = edge_of(&doc, body);
        let id = command(
            &mut doc,
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
                "type": "balloon", "picks": [{"body": body, "occurrence": occurrence, "edge": edge}]
            }),
        )["annotation"]
            .as_u64()
            .unwrap();
        assert_eq!(annotation(&doc, id)["label"], "2");
        balloons.push(id);
    }
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": copy["component"], "material": "aluminum"}),
    );
    assert_eq!(
        annotation(&doc, list)["label"],
        "1 Pin x1; 2 Plate x2; 3 PlateCopy x1"
    );
    assert_eq!(annotation(&doc, balloons[0])["label"], "2");
    assert_eq!(annotation(&doc, balloons[1])["label"], "3");
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(annotation(&doc, list)["label"], "1 Pin x1; 2 Plate x3");
    let saved = doc.to_json();
    let kernel = MockKernel::default();
    kernel.measures.set(true);
    let mut loaded = Document::from_json(&saved, kernel).unwrap();
    assert_eq!(annotation(&loaded, list)["table"]["group_identical"], true);
    // Reading definitions does not evaluate geometry; reopen through the
    // normal recompute command before checking the generated table/balloons.
    command(&mut loaded, json!({"cmd": "recompute"}));
    assert_eq!(loaded.bodies().len(), 3);
    assert_eq!(loaded.instances().len(), 4);
    assert_eq!(annotation(&loaded, list)["label"], "1 Pin x1; 2 Plate x3");
    for balloon in &balloons {
        assert_eq!(annotation(&loaded, *balloon)["label"], "2");
    }
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": list,
        "table": {"group_identical": false}}),
    );
    assert_eq!(
        annotation(&doc, list)["label"],
        "1 Pin x1; 2 Plate x2; 3 PlateCopy x1"
    );
}

/// A grouped parts list counts the placements of a component it cannot
/// measure in one row (mitcad#1315): the mock kernel's measures are off, so
/// no placement has a signature to group by.
#[test]
fn grouped_parts_list_counts_an_unmeasured_components_placements_once() {
    let (mut doc, [plate, second, _]) = assembly();
    let third = command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": plate,
               "transform": {"translation": [100, 0, 0]}}),
    )["occurrence"]
        .as_str()
        .unwrap()
        .to_owned();
    // The view takes the third placement in (mitcad#1293).
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "subject": {"type": "design"}}),
    );
    let list = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
            "type": "parts_list", "at": [300, 280], "table": {"group_identical": true}
        }),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(annotation(&doc, list)["label"], "1 Pin x1; 2 Plate x3");
    // One number for the three Plates.
    let edge = edge_of(&doc, "F2.b0");
    for occurrence in [&plate, &second, &third] {
        let balloon = command(
            &mut doc,
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
                "type": "balloon", "picks": [{"body": "F2.b0", "occurrence": occurrence, "edge": edge}]
            }),
        )["annotation"]
            .as_u64()
            .unwrap();
        assert_eq!(annotation(&doc, balloon)["label"], "2", "{occurrence}");
    }
}

#[test]
fn grouped_drawing_parts_use_materials_in_the_actual_assembly_paths() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    for (parent, child, material, sketch) in [
        ("SteelParent", "SteelChild", "steel", "F1"),
        ("AluminumParent", "AluminumChild", "aluminum", "F3"),
    ] {
        command(&mut doc, json!({"cmd": "create_component", "name": parent}));
        command(
            &mut doc,
            json!({"cmd": "set_component_material", "component": parent, "material": material}),
        );
        command(&mut doc, json!({"cmd": "create_component", "name": child}));
        command(
            &mut doc,
            json!([
                {"cmd": "sketch.create"},
                {"cmd": "sketch.add_rectangle", "sketch": sketch, "corner": [0, 0], "width": 10, "height": 10},
                {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": sketch, "region": R}],
                    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
                {"cmd": "activate_component", "component": "C0"}
            ]),
        );
    }
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base",
        "direction": "top", "at": [150, 150], "scale": "1:1"}),
    );
    let list = command(&mut doc, json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
        "type": "parts_list", "at": [300, 280], "table": {"levels": "parts", "group_identical": true}
    }))["annotation"].as_u64().unwrap();
    assert_eq!(
        annotation(&doc, list)["label"],
        "1 AluminumChild x1; 2 SteelChild x1"
    );
    // The bodies have no own material; querying them without their paths
    // gives steel for both and would have merged them incorrectly.
    for body in doc.bodies() {
        assert_eq!(doc.body_material(body.uid, None).0.id, "steel");
    }
    let bom = query(
        &doc,
        json!({"query": "parts_list", "group_identical": true}),
    );
    let physical: Vec<_> = bom["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["name"] == "SteelChild" || r["name"] == "AluminumChild")
        .collect();
    assert_eq!(physical.len(), 2, "{bom}");
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "AluminumParent", "material": "steel"}),
    );
    assert_eq!(annotation(&doc, list)["label"], "1 SteelChild x2");
}

#[test]
fn grouping_subject_own_bodies_keeps_ancestor_material_and_placement_balloon_identity() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Parent"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "Parent", "material": "aluminum"}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "OwnBodies"}),
    );
    for x in [0, 20] {
        command(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "box",
            "plane": {"type": "origin", "plane": "xy"}, "corner": [x, 0],
            "length": 10, "width": 10, "height": 5, "operation": "new_body"}}),
        );
    }
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F1.b0", "material": "steel"}),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base",
        "direction": "top", "at": [150, 150], "scale": "1:1",
        "subject": {"type": "occurrence", "occurrence": "O1/O2"}}),
    );
    let list = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
            "type": "parts_list", "at": [300, 280], "table": {"group_identical": true}
        }),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(annotation(&doc, list)["label"], "1 Body1 x1; 2 Body2 x1");
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F1.b0", "material": null}),
    );
    assert_eq!(annotation(&doc, list)["label"], "1 Body1 x2");
}

#[test]
fn one_component_in_two_material_paths_has_distinct_grouped_balloon_numbers() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().measures.set(true);
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "SteelParent"}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Shared"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box",
        "plane": {"type": "origin", "plane": "xy"}, "corner": [0, 0],
        "length": 10, "width": 10, "height": 5, "operation": "new_body"}}),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "create_component", "name": "AluminumParent"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_component_material", "component": "AluminumParent", "material": "aluminum"}),
    );
    command(
        &mut doc,
        json!({"cmd": "copy_occurrence", "occurrence": "O1/O2"}),
    );
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_property", "owner": "Shared", "set": "Identity", "key": "Material", "value": "Catalogue label"}),
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base",
        "direction": "top", "at": [150, 150], "scale": "1:1"}),
    );
    let list = command(&mut doc, json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
        "type": "parts_list", "at": [300, 280], "table": {"levels": "parts", "group_identical": true,
            "columns": ["item", "name", "quantity", "material", "Computed/Material", "Identity/Material"]}
    }))["annotation"].as_u64().unwrap();
    assert_eq!(annotation(&doc, list)["label"], "1 Shared x1; 2 Shared x1");
    let steel = doc.material("steel").unwrap().name.clone();
    let aluminum = doc.material("aluminum").unwrap().name.clone();
    let cells = texts(&doc);
    assert!(cells.contains(&steel), "{cells:?}");
    assert!(cells.contains(&aluminum), "{cells:?}");
    assert!(cells.contains(&"Catalogue label".to_owned()), "{cells:?}");
    let edge = edge_of(&doc, "F1.b0");
    for (occurrence, number) in [("O1/O2", "1"), ("O3/O4", "2")] {
        let balloon = command(&mut doc, json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
            "type": "balloon", "picks": [{"body": "F1.b0", "occurrence": occurrence, "edge": edge}]
        }))["annotation"].as_u64().unwrap();
        assert_eq!(annotation(&doc, balloon)["label"], number);
    }
    let bom = query(
        &doc,
        json!({"query": "parts_list", "group_identical": true}),
    );
    let shared: Vec<_> = bom["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["name"] == "Shared")
        .collect();
    assert_eq!(shared.len(), 2, "{bom}");
    assert!(shared.iter().all(|row| row["quantity"] == 1));
    // The ungrouped component row counts both placements, so its Material
    // cell lists both effective materials instead of just the base context.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": list,
        "table": {"group_identical": false}}),
    );
    assert_eq!(annotation(&doc, list)["label"], "1 Shared x2");
    assert!(texts(&doc).contains(&format!("{aluminum}, {steel}")));
}
