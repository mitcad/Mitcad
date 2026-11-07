// SPDX-License-Identifier: MIT
//! The drawing features of mitcad#729 on the mock kernel: a drawing's own
//! decimals and letters' height for the dimensions and annotations without
//! their own; lines, arcs and texts drawn on a sheet or in a view, outside
//! the model; a centre pattern made from one circle and edge extensions;
//! a view turned on the sheet with its annotations, details and breaks; a
//! view turned to another direction (a base view's, a projected view's
//! side, a section looking the other way, the projection method) with the
//! sections and details drawn on it following; the file form and old files.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::ids::BodyUid;
use mitcad_model::internal::profile::dot;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_drawing::def::{Drawing, ViewKind};

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

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn point(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

fn answer(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))
}

fn sheet(doc: &Document<MockKernel>) -> Value {
    answer(doc)["sheets"][0].clone()
}

/// The first sheet's annotation of this id in the `drawing` query.
fn annotation(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}: {}", sheet(doc)["annotations"]))
}

/// A view of the first sheet in the `drawing` query.
fn view(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc)["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no view {id}"))
}

/// The group `A<id>` of the first sheet.
fn group(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == format!("A{id}").as_str())
        .cloned()
        .unwrap_or_else(|| panic!("no group of annotation {id}"))
}

/// The heights of a group's texts.
fn heights(group: &Value) -> Vec<f64> {
    group["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i.get("text").and(i["height"].as_f64()))
        .collect()
}

/// The middle of a view's box on the sheet.
fn box_middle(view: &Value) -> [f64; 2] {
    let (a, b) = (point(&view["box"][0]), point(&view["box"][1]));
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

fn pick(edge: &str) -> Value {
    json!({"body": "F2.b0", "edge": edge})
}

/// A 40 x 30 x 20 block at the origin and a 20 x 10 x 10 block beside it,
/// the kernel listing the first block's edges: the front's left side
/// (x 0) and right side (x 40.1234, so that decimals show), four holes of
/// radius 2 on its top on a circle of 8 about (20, 15), and a straight
/// edge above the front at z 25 from x 5 to 30 (to run on to the left
/// side's line); a drawing with a front view at 1:1 (the block's front
/// from 65, 110 on the sheet) and the top view above it. Returns the
/// edges' names.
fn drawn() -> (Document<MockKernel>, Vec<String>) {
    let mut doc = Document::new(MockKernel::default());
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
    doc.kernel().lists_geometry.set(true);
    let body: BodyUid = "F2.b0".parse().unwrap();
    let names: Vec<String> = doc
        .kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .collect();
    assert!(names.len() >= 7, "{names:?}");
    {
        let mut curves = doc.kernel().edge_curves.borrow_mut();
        curves.insert(
            names[0].clone(),
            CurveGeometry::Line {
                start: [0.0, 0.0, 0.0],
                end: [0.0, 0.0, 20.0],
            },
        );
        curves.insert(
            names[1].clone(),
            CurveGeometry::Line {
                start: [40.1234, 0.0, 0.0],
                end: [40.1234, 0.0, 20.0],
            },
        );
        for (k, (dx, dy)) in [(8.0, 0.0), (0.0, 8.0), (-8.0, 0.0), (0.0, -8.0)]
            .into_iter()
            .enumerate()
        {
            let center = [20.0 + dx, 15.0 + dy, 20.0];
            let rim = [center[0] + 2.0, center[1], 20.0];
            curves.insert(
                names[2 + k].clone(),
                CurveGeometry::Circle {
                    center,
                    normal: [0.0, 0.0, 1.0],
                    radius: 2.0,
                    start: rim,
                    end: rim,
                },
            );
        }
        curves.insert(
            names[6].clone(),
            CurveGeometry::Line {
                start: [5.0, 0.0, 25.0],
                end: [30.0, 0.0, 25.0],
            },
        );
    }
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
    (doc, names)
}

#[test]
fn a_drawings_decimals_and_letters_are_every_annotations_unless_it_has_its_own() {
    let (mut doc, names) = drawn();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "picks": [pick(&names[0]), pick(&names[1])]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "text", "sheet": 0,
               "at": [30, 40], "text": "GENERAL TOLERANCES ISO 2768-m"}),
    );
    // The engine's decimals and 3.5 mm letters before.
    assert_eq!(annotation(&doc, 1)["label"], "40.123");
    assert_eq!(heights(&group(&doc, 1)), [3.5]);
    assert!(answer(&doc)["decimals"].is_null());
    // One decimal and 5 mm letters for the drawing.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing", "drawing": "D1", "decimals": 1, "text_height": 5}),
    );
    assert_eq!(undo_label(&doc), "Edit Drawing1");
    assert_eq!(answer(&doc)["decimals"], 1);
    assert_eq!(answer(&doc)["text_height"], 5.0);
    assert_eq!(annotation(&doc, 1)["label"], "40.1");
    assert_eq!(heights(&group(&doc, 1)), [5.0]);
    assert_eq!(heights(&group(&doc, 2)), [5.0]);
    // Its own win: two decimals, 2.5 mm letters; the note's own height.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 1,
               "format": {"decimals": 2}, "height": 2.5}),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 2, "height": 7}),
    );
    assert_eq!(annotation(&doc, 1)["label"], "40.12");
    assert_eq!(annotation(&doc, 1)["height"], 2.5);
    assert_eq!(heights(&group(&doc, 1)), [2.5]);
    assert_eq!(heights(&group(&doc, 2)), [7.0]);
    // `null`: the drawing's again.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 1,
               "height": null}),
    );
    assert_eq!(heights(&group(&doc, 1)), [5.0]);
    // A table's letters and rows follow too.
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "hole_table",
               "at": [250, 280]}),
    );
    let table = group(&doc, 3);
    assert!(heights(&table).iter().all(|h| *h > 3.5 - 1e-9), "{table}");
    // Saved and read again.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["drawings"][0]["defaults"],
        json!({"decimals": 1, "text_height": 5.0})
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    // Back to none, undone.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing", "drawing": "D1", "decimals": null, "text_height": null}),
    );
    assert!(answer(&doc)["text_height"].is_null());
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert!(
        file["drawings"][0].get("defaults").is_none(),
        "left out when none"
    );
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(answer(&doc)["decimals"], 1);
    // Refused.
    for (cmd, message) in [
        (
            json!({"cmd": "edit_drawing", "drawing": "D1", "decimals": 9}),
            "at most 6 decimals",
        ),
        (
            json!({"cmd": "edit_drawing", "drawing": "D1", "text_height": 0}),
            "high",
        ),
        (
            json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": 1,
                   "height": -1}),
            "high",
        ),
    ] {
        let error = refused(&mut doc, cmd);
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn lines_arcs_and_texts_are_drawn_on_the_sheet_and_in_views() {
    let (mut doc, _) = drawn();
    // A line on the sheet, an arc and a text in the front view.
    let line = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "line", "sheet": 0,
               "points": [[20, 20], [60, 20]], "pen": "thick"}),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(undo_label(&doc), format!("Add line {line} to Drawing1"));
    // About the block's front corner (65, 110 on the sheet), from its
    // right counter-clockwise to above it.
    let arc = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "arc",
               "points": [[65, 110], [75, 110], [65, 120]]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let text = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "text",
               "at": [70, 100], "text": "DATUM PLANE"}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let entry = annotation(&doc, arc);
    assert_eq!(entry["type"], "arc");
    assert_eq!(entry["view"], 1);
    assert_eq!(entry["broken"], false);
    // Kept in the view's coordinates: the corner is its origin.
    assert!(near(entry["radius"].as_f64().unwrap(), 10.0), "{entry}");
    assert!(near(point(&entry["center"])[0], 0.0), "{entry}");
    assert!(near(
        entry["end"].as_f64().unwrap() - entry["start"].as_f64().unwrap(),
        std::f64::consts::FRAC_PI_2
    ));
    assert_eq!(annotation(&doc, text)["label"], "DATUM PLANE");
    assert_eq!(annotation(&doc, line)["view"], Value::Null);
    assert_eq!(group(&doc, line)["class"], "sketch");
    assert_eq!(group(&doc, line)["items"][0]["pen"], "thick");
    let ids: Vec<u64> = view(&doc, 1)["sketch"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    assert_eq!(ids, [arc, text]);
    // The view moved: its arc and text with it, the sheet's line stays.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "move": [10, -5]}),
    );
    let moved = group(&doc, arc)["items"][0]["arc"]["center"].clone();
    assert!(
        near(point(&moved)[0], 75.0) && near(point(&moved)[1], 105.0),
        "{moved}"
    );
    assert_eq!(
        group(&doc, line)["items"][0]["line"],
        json!([[20.0, 20.0], [60.0, 20.0]])
    );
    // At 2:1 the arc is twice as large, the text as high.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "scale": "2:1"}),
    );
    assert!(near(
        group(&doc, arc)["items"][0]["arc"]["radius"]
            .as_f64()
            .unwrap(),
        20.0
    ));
    assert_eq!(heights(&group(&doc, text)), [3.5]);
    // Edited: dragged, its pen, its text.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": line,
               "move": [0, 5]}),
    );
    assert_eq!(undo_label(&doc), format!("Move line {line} of Drawing1"));
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": line,
               "pen": "chain"}),
    );
    assert_eq!(
        group(&doc, line)["items"][0]["line"],
        json!([[20.0, 25.0], [60.0, 25.0]])
    );
    assert_eq!(group(&doc, line)["items"][0]["pen"], "chain");
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": text,
               "text": "A\nB", "height": 5}),
    );
    assert_eq!(heights(&group(&doc, text)), [5.0, 5.0]);
    // Saved, read again.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let sheet_file = &file["drawings"][0]["sheets"][0];
    assert_eq!(sheet_file["sketch"][0]["type"], "line");
    assert_eq!(sheet_file["views"][0]["sketch"][1]["type"], "text");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    // Refused: what they have not, too few points, a text on the sheet in
    // the sketch list.
    for (cmd, message) in [
        (
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "arc", "sheet": 0,
                   "points": [[0, 0], [1, 0]]}),
            "center, start and end",
        ),
        (
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "line", "view": 1,
                   "points": [[0, 0], [1, 0]], "picks": [[65, 120]]}),
            "outside the model",
        ),
        (
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "line", "sheet": 0,
                   "points": [[5, 5], [5, 5]]}),
            "length",
        ),
        (
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "text", "view": 1,
                   "at": [5, 5], "text": "X", "pen": "thick"}),
            "no pen",
        ),
        (
            json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": line,
                   "text": "X"}),
            "no text",
        ),
        (
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
                   "picks": [[65, 120], [105, 121]], "points": [[0, 0], [1, 1]]}),
            "no points or pen",
        ),
    ] {
        let error = refused(&mut doc, cmd);
        assert!(error.contains(message), "{error}");
    }
    let mut bad: Value = serde_json::from_str(&saved).unwrap();
    bad["drawings"][0]["sheets"][0]["sketch"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type": "text", "id": 99, "text": "X", "at": [1, 1]}));
    let Err(error) = Document::from_json(&bad.to_string(), MockKernel::default()) else {
        panic!("a text in the sheet's sketch was read");
    };
    assert!(error.to_string().contains("one of its notes"), "{error}");
    // Deleted with the annotations' command, and with their view.
    command(
        &mut doc,
        json!({"cmd": "delete_drawing_annotation", "drawing": "D1", "annotation": line}),
    );
    command(
        &mut doc,
        json!({"cmd": "delete_drawing_view", "drawing": "D1", "view": 1}),
    );
    let left = sheet(&doc)["annotations"].as_array().unwrap().len();
    assert_eq!(left, 0, "{}", sheet(&doc)["annotations"]);
}

/// The part of an SVG file of the group `A<id>`, up to its end.
fn svg_group(svg: &str, id: u64) -> &str {
    let open = format!(r#"id="A{id}""#);
    let start = svg
        .find(&open)
        .unwrap_or_else(|| panic!("no group A{id} in {svg}"));
    let end = svg[start..].find("</g>").map_or(svg.len(), |e| start + e);
    &svg[start..end]
}

#[test]
fn an_extra_thick_line_and_arc_are_drawn_0_7_mm_wide_and_kept() {
    // mitcad#1402: ISO 128's extra wide line, twice the wide one.
    let (mut doc, _) = drawn();
    let line = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "line", "sheet": 0,
               "points": [[20, 20], [60, 20]], "pen": "extra_thick"}),
    )["annotation"]
        .as_u64()
        .unwrap();
    let arc = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "arc",
               "points": [[65, 110], [75, 110], [65, 120]], "pen": "extra_thick"}),
    )["annotation"]
        .as_u64()
        .unwrap();
    // A thin line made extra thick by an edit.
    let edited = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "type": "line", "sheet": 0,
               "points": [[20, 30], [60, 30]]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(group(&doc, edited)["items"][0]["pen"], "thin");
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": edited,
               "pen": "extra_thick"}),
    );
    for id in [line, arc, edited] {
        assert_eq!(annotation(&doc, id)["pen"], "extra_thick", "{id}");
        assert_eq!(group(&doc, id)["items"][0]["pen"], "extra_thick", "{id}");
    }
    // The pens' table gives its width and no dashes.
    let pens = sheet(&doc)["pens"].clone();
    assert!(
        near(pens["extra_thick"]["width"].as_f64().unwrap(), 0.7),
        "{pens}"
    );
    assert_eq!(pens["extra_thick"]["dashes"], json!([]), "{pens}");
    assert!(near(pens["thick"]["width"].as_f64().unwrap(), 0.35));

    // Saved and read again: the pen is kept by its name.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let sheet_file = &file["drawings"][0]["sheets"][0];
    assert_eq!(sheet_file["sketch"][0]["pen"], "extra_thick");
    assert_eq!(sheet_file["sketch"][1]["pen"], "extra_thick");
    assert_eq!(sheet_file["views"][0]["sketch"][0]["pen"], "extra_thick");
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    for id in [line, arc, edited] {
        assert_eq!(group(&loaded, id)["items"][0]["pen"], "extra_thick", "{id}");
    }

    // The writers draw it 0.7 mm wide; the DXF file on its own layer.
    let dir = mitcad_model::testing::unique_temp_dir("extra-thick");
    let svg = dir.join("extra.svg");
    let pdf = dir.join("extra.pdf");
    let dxf = dir.join("extra.dxf");
    for path in [&svg, &pdf, &dxf] {
        command(
            &mut loaded,
            json!({"cmd": "export_drawing", "drawing": "D1", "path": path.to_string_lossy()}),
        );
    }
    let text = std::fs::read_to_string(&svg).unwrap();
    for id in [line, arc, edited] {
        let g = svg_group(&text, id);
        assert!(g.contains(r#"stroke-width="0.7""#), "{id}: {g}");
    }
    let bytes = std::fs::read(&pdf).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("0.7 w [] 0 d"), "no 0.7 mm pen in the PDF");
    let text = std::fs::read_to_string(&dxf).unwrap();
    let entities = text
        .find("ENTITIES")
        .map(|at| &text[at..])
        .expect("the DXF file's entities");
    assert!(
        text[..text.len() - entities.len()].contains("  2\nEXTRA_THICK\n"),
        "no EXTRA_THICK layer"
    );
    let on_layer = entities.matches("  8\nEXTRA_THICK\n").count();
    assert!(on_layer >= 3, "{on_layer} entities on EXTRA_THICK");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_centre_pattern_marks_every_hole_of_a_pattern_in_one_step() {
    let (mut doc, names) = drawn();
    // One hole picked on the top view, by its name: all four found.
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2,
               "type": "center_pattern", "picks": [pick(&names[3])]}),
    );
    let id = added["annotation"].as_u64().unwrap();
    assert_eq!(
        undo_label(&doc),
        format!("Add center pattern {id} to Drawing1")
    );
    let entry = annotation(&doc, id);
    assert_eq!(entry["broken"], false, "{entry}");
    assert_eq!(entry["picks"].as_array().unwrap().len(), 4, "{entry}");
    assert_eq!(
        entry["picks"][0]["edge"],
        names[3].as_str(),
        "the one picked first"
    );
    // The circle through their centres, about the pattern's centre.
    let place = point(&view(&doc, 2)["place"]);
    let marks = group(&doc, id);
    assert_eq!(marks["class"], "center-mark");
    let pitch = marks["items"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|i| i.get("arc").cloned())
        .unwrap_or_else(|| panic!("no circle through the holes: {marks}"));
    assert!(near(pitch["radius"].as_f64().unwrap(), 8.0), "{pitch}");
    let c = point(&pitch["center"]);
    assert!(
        near(c[0], place[0] + 20.0) && near(c[1], place[1] + 15.0),
        "{pitch}"
    );
    // By a click on a hole's rim too.
    let rim = [place[0] + 28.0 + 2.0, place[1] + 15.0];
    let clicked = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2,
               "type": "center_pattern", "picks": [rim]}),
    );
    let second = annotation(&doc, clicked["annotation"].as_u64().unwrap());
    assert_eq!(second["picks"].as_array().unwrap().len(), 4, "{second}");
    // A hole gone (its edge no longer a circle): the pattern is broken.
    doc.kernel().edge_curves.borrow_mut().remove(&names[5]);
    let entry = annotation(&doc, id);
    assert_eq!(entry["broken"], true, "a pick gone breaks it: {entry}");
    // Not on a straight edge.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
               "type": "center_pattern", "picks": [pick(&names[0])]}),
    );
    assert!(error.contains("circle"), "{error}");
}

#[test]
fn edge_extensions_run_edges_on_to_where_they_meet() {
    let (mut doc, names) = drawn();
    // The left side (x 0, z 0..20) and the edge at z 25 from x 5: their
    // lines meet at x 0, z 25, on the sheet at 65, 135.
    let id = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
               "type": "edge_extension", "picks": [pick(&names[0]), pick(&names[6])]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    assert_eq!(
        undo_label(&doc),
        format!("Add edge extension {id} to Drawing1")
    );
    let lines: Vec<[[f64; 2]; 2]> = group(&doc, id)["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i.get("line"))
        .map(|l| [point(&l[0]), point(&l[1])])
        .collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    // Up the left side from its top to 2 mm past the corner, and along
    // the edge from its left end.
    assert!(
        near(lines[0][0][1], 131.0) && near(lines[0][1][1], 137.0),
        "{lines:?}"
    );
    assert!(
        near(lines[1][0][0], 69.0) && near(lines[1][1][0], 63.0),
        "{lines:?}"
    );
    // One edge: on beyond its ends by its length.
    let one = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
               "type": "edge_extension", "picks": [pick(&names[6])]}),
    )["annotation"]
        .as_u64()
        .unwrap();
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": one,
               "line_offset": 4}),
    );
    let b = annotation(&doc, one)["box"].clone();
    assert!(
        near(point(&b[0])[0], 70.0 - 5.0) && near(point(&b[1])[0], 95.0 + 5.0),
        "{b}"
    );
    // Parallel edges are refused.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
               "type": "edge_extension", "picks": [pick(&names[0]), pick(&names[1])]}),
    );
    assert!(error.contains("parallel"), "{error}");
}

#[test]
fn a_view_turned_on_the_sheet_takes_its_annotations_along() {
    let (mut doc, names) = drawn();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "linear",
               "picks": [pick(&names[0]), pick(&names[1])]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "line",
               "points": [[65, 110], [75, 110]]}),
    );
    let middle = box_middle(&view(&doc, 1));
    let wide = point(&view(&doc, 1)["box"][1])[0] - point(&view(&doc, 1)["box"][0])[0];
    // A quarter turn to the left.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "rotation": 90}),
    );
    assert_eq!(undo_label(&doc), "Edit view 1 of Drawing1");
    let front = view(&doc, 1);
    assert_eq!(front["rotation"], 90.0);
    // Its box turned about its middle: as high as it was wide.
    let after = box_middle(&front);
    assert!(
        near(after[0], middle[0]) && near(after[1], middle[1]),
        "{front}"
    );
    let high = point(&front["box"][1])[1] - point(&front["box"][0])[1];
    assert!(near(high, wide), "{front}");
    // The horizontal dimension is a vertical one now, as long.
    let dimension = annotation(&doc, 1);
    assert_eq!(dimension["along"], "vertical", "{dimension}");
    assert!(
        near(dimension["value"].as_f64().unwrap(), 40.1234),
        "{dimension}"
    );
    assert!(dimension.get("turn").is_none());
    // The line drawn in it turned with it: upright now.
    let line = group(&doc, 2)["items"][0]["line"].clone();
    assert!(near(point(&line[0])[0], point(&line[1])[0]), "{line}");
    // The view above it is still drawn, from its new side.
    assert!(view(&doc, 2).get("error").is_none());
    // Thirty degrees more: the dimension keeps its value and turns.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "rotation": 120}),
    );
    let dimension = annotation(&doc, 1);
    assert!(
        near(dimension["value"].as_f64().unwrap(), 40.1234),
        "{dimension}"
    );
    assert!(
        near(dimension["turn"].as_f64().unwrap(), 30.0),
        "{dimension}"
    );
    // Saved and read again with it.
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["drawings"][0]["sheets"][0]["views"][0]["rotation"],
        120.0
    );
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    // Back to none, undone.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "rotation": 0}),
    );
    let dimension = annotation(&doc, 1);
    assert_eq!(dimension["along"], "horizontal", "{dimension}");
    assert!(dimension.get("turn").is_none());
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(view(&doc, 1)["rotation"], 120.0);
}

#[test]
fn a_turned_views_details_and_breaks_turn_with_it() {
    let (mut doc, _) = drawn();
    let detail = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "detail", "parent": 1,
               "center": [75, 115], "radius": 4, "at": [300, 60]}),
    )["view"]
        .as_u64()
        .unwrap();
    // A detail turns with its parent only.
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": detail, "rotation": 90}),
    );
    assert!(error.contains("turns with its parent"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "rotation": 90}),
    );
    // Its circle turned with the view: it shows the same corner.
    let d = doc.find_drawing("D1").unwrap();
    let ViewKind::Detail { center, .. } = &d.sheets[0].view(detail as u32).unwrap().kind else {
        panic!("a detail");
    };
    assert!(near(center[0], -5.0) && near(center[1], 10.0), "{center:?}");
    // A break: quarter turns carry it over, other turns are refused.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "rotation": 0}),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1,
               "add_break": {"direction": "horizontal", "from": [75, 120], "to": [85, 120]}}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "rotation": 45}),
    );
    assert!(error.contains("quarter turns"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "rotation": -90}),
    );
    let breaks = view(&doc, 1)["breaks"].clone();
    assert_eq!(breaks[0]["direction"], "vertical", "{breaks}");
    assert!(
        near(breaks[0]["from"].as_f64().unwrap(), -20.0)
            && near(breaks[0]["to"].as_f64().unwrap(), -10.0),
        "{breaks}"
    );
}

#[test]
fn a_view_turned_another_way_takes_its_sections_and_details_along() {
    let (mut doc, _) = drawn();
    // A section through the block's middle, upright on the front view; a
    // detail on the top view; an annotation on the section.
    let section = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[85, 100], [85, 140]], "at": [250, 120]}),
    )["view"]
        .as_u64()
        .unwrap() as u32;
    let detail = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "detail", "parent": 2,
               "center": [75, 160], "radius": 4, "at": [300, 60]}),
    )["view"]
        .as_u64()
        .unwrap() as u32;
    let plane = |doc: &Document<MockKernel>| {
        let d: &Drawing = doc.find_drawing("D1").unwrap();
        let sheet = &d.sheets[0];
        let ViewKind::Section {
            origin,
            normal,
            span,
            ..
        } = &sheet.view(section).unwrap().kind
        else {
            panic!("a section");
        };
        let (eye, x) = sheet.axes(1).unwrap();
        (*origin, *normal, *span, eye, x)
    };
    let (origin, _, span, eye, x) = plane(&doc);
    let before = (dot(origin, x), *span.as_ref().unwrap());
    let circle = |doc: &Document<MockKernel>| {
        let d = doc.find_drawing("D1").unwrap();
        match &d.sheets[0].view(detail).unwrap().kind {
            ViewKind::Detail { center, .. } => *center,
            _ => panic!("a detail"),
        }
    };
    let center = circle(&doc);
    // The front view turned to look from the right (its direction): the
    // section's line stays where it was drawn on it, edge on.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "direction": "right"}),
    );
    let (origin, normal, span, new_eye, new_x) = plane(&doc);
    assert!(!near(dot(eye, new_eye), 1.0), "the view looks another way");
    assert!(near(dot(normal, new_eye), 0.0), "edge on: {normal:?}");
    assert!(near(dot(origin, new_x), before.0), "{origin:?}");
    assert_eq!(span.unwrap(), before.1);
    // The section and the detail are still drawn. The top view above the
    // front view now looks down with the model's y to the right: turned a
    // quarter to the right in its own plane, so the detail's circle turned
    // with it and shows the same place of the model.
    for id in [section, detail] {
        let v = view(&doc, u64::from(id));
        assert!(v.get("error").is_none(), "{v}");
    }
    let turned = circle(&doc);
    assert!(
        near(turned[0], center[1]) && near(turned[1], -center[0]),
        "{turned:?}"
    );
    // The projected view's side and the projection method: it looks
    // another way each time, and the detail keeps its circle where it is
    // drawn on it.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 2, "side": "below"}),
    );
    assert_eq!(circle(&doc), turned);
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_sheet", "drawing": "D1", "projection": "first_angle"}),
    );
    assert_eq!(circle(&doc), turned);
    // The section looks the other way: a section of it follows too.
    let inner = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section",
               "parent": section, "line": [[250, 100], [250, 140]], "at": [330, 200]}),
    )["view"]
        .as_u64()
        .unwrap();
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": section, "flip": true}),
    );
    assert!(view(&doc, inner).get("error").is_none());
    // Undone step by step to where it started.
    for _ in 0..5 {
        command(&mut doc, json!({"cmd": "undo"}));
    }
    let (origin, _, _, eye_again, _) = plane(&doc);
    assert!(near(dot(eye_again, eye), 1.0));
    assert!(near(dot(origin, x), before.0));
    assert_eq!(circle(&doc), center);
}

#[test]
fn files_without_the_new_fields_read_as_before() {
    // A drawing as files had it before mitcad#729: no defaults, no
    // rotation, nothing drawn outside the model.
    let old = json!({
        "uid": "D1",
        "name": "Drawing1",
        "sheets": [{"name": "Sheet1", "size": "A3", "views": [
            {"id": 1, "kind": {"type": "base", "direction": "front"}, "place": [100, 120],
             "scale": 1.0, "annotations": [{"id": 1, "type": "linear", "along": "horizontal",
             "picks": [{"body": "F2.b0", "edge": "E{F2:start|F2:side(c1[c4,c2])}"},
                       {"body": "F2.b0", "edge": "E{F2:end|F2:side(c1[c4,c2])}"}]}]}
        ], "notes": [{"id": 2, "text": "NOTE", "at": [10, 10]}]}]
    });
    let drawing: Drawing = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(drawing.defaults, Default::default());
    let view = &drawing.sheets[0].views[0];
    assert_eq!(view.rotation, 0.0);
    assert!(view.sketch.is_empty() && drawing.sheets[0].sketch.is_empty());
    assert_eq!(view.annotations[0].height, None);
    assert_eq!(view.annotations[0].turn, None);
    drawing.check().unwrap();
    // Written again as it was read: the new fields left out.
    let again = serde_json::to_value(&drawing).unwrap();
    for field in ["defaults", "rotation", "sketch", "height", "turn"] {
        assert!(
            !again.to_string().contains(&format!("\"{field}\"")),
            "{field}: {again}"
        );
    }
}
