// SPDX-License-Identifier: MIT
//! Cut sheets (mitcad#853) through the JSON api on the mock kernel, whose
//! projections are the bodies' boxes and whose lines of sight cross the
//! boxes: a cut sheet added with its parts, a part added with a count,
//! the outline following a parameter, a deleted body marked missing,
//! overlaps and parts off the sheet, moves and turns (also through the
//! drawing commands), the face chosen, undo, the design file, and the DXF
//! and SVG files read back with their layers and sizes. The outlines'
//! own geometry (holes, slots) is tested in
//! `core/model-drawing/src/cut_sheet_tests.rs` and through OCCT in
//! `tools/cli/tests/cut_sheet.json`.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::LARGEST_U32_ID;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

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

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

/// A plate `len` (100) x 60 x 3 (F1.b0, Body1) and a block 40 x 30 x 5
/// beside it (F2.b0, Body2), boxes whose faces the mock kernel measures.
fn plates() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_faces.set(true);
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "len", "value": "100 mm"},
            {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
              "length": "len", "width": 60, "height": 3, "operation": "new_body"}},
            {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [200, 0],
              "length": 40, "width": 30, "height": 5, "operation": "new_body"}},
        ]),
    );
    doc
}

fn cut_sheet(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "cut_sheet", "drawing": "D1"}))
}

fn part(doc: &Document<MockKernel>, id: u64) -> Value {
    cut_sheet(doc)["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no part {id}"))
}

/// A part's box on the sheet: lower left and upper right.
fn sheet_box(part: &Value) -> [[f64; 2]; 2] {
    let p = |v: &Value| [number(&v[0]), number(&v[1])];
    [p(&part["box"][0]), p(&part["box"][1])]
}

#[test]
fn a_cut_sheet_is_a_drawing_of_its_own_kind() {
    let mut doc = plates();
    let added = command(&mut doc, json!({"cmd": "add_cut_sheet"}));
    assert_eq!(added["drawing"], "D1");
    assert_eq!(added["name"], "CutSheet1");
    assert_eq!(added["parts"], json!([]));
    assert_eq!(undo_label(&doc), "Add CutSheet1");
    let list = query(&doc, json!({"query": "drawings"}));
    assert_eq!(list[0]["kind"], "cut_sheet");
    assert_eq!(list[0]["sheets"][0]["size"], "600 x 300 mm");
    // The material's default size; at 1:1, nothing of a drawing's own.
    let sheet = cut_sheet(&doc);
    assert_eq!(
        (number(&sheet["width"]), number(&sheet["height"])),
        (600.0, 300.0)
    );
    let drawn = query(&doc, json!({"query": "drawing", "drawing": "D1"}));
    assert_eq!(drawn["kind"], "cut_sheet");
    assert_eq!(drawn["sheets"][0]["scale"], "1:1");
    assert_eq!(drawn["sheets"][0]["groups"], json!([]));
    // The material typed in.
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet", "drawing": "CutSheet1", "width": 1000, "height": 500}),
    );
    assert_eq!(undo_label(&doc), "Edit CutSheet1");
    assert_eq!(number(&cut_sheet(&doc)["width"]), 1000.0);
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_cut_sheet", "drawing": "D1", "width": 0}),
    );
    assert!(error.contains("at most"), "{error}");
    // A drawing's views, sheets and notes are not a cut sheet's.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base"}),
    );
    assert!(error.contains("is a cut sheet"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_sheet", "drawing": "D1"}),
    );
    assert!(error.contains("is a cut sheet"), "{error}");
    // A drawing is no cut sheet.
    command(&mut doc, json!({"cmd": "add_drawing"}));
    let error = refused(
        &mut doc,
        json!({"cmd": "add_cut_sheet_parts", "drawing": "Drawing1", "bodies": ["F1.b0"]}),
    );
    assert!(error.contains("not a cut sheet"), "{error}");
    assert_eq!(
        query(&doc, json!({"query": "drawings"}))[1]["kind"],
        "drawing"
    );
}

#[test]
fn a_part_with_a_count_of_two_is_two_views_at_one_to_one() {
    let mut doc = plates();
    let added = command(
        &mut doc,
        json!({"cmd": "add_cut_sheet", "bodies": ["Body1"], "count": 2}),
    );
    assert_eq!(added["parts"], json!([1, 2]));
    let sheet = cut_sheet(&doc);
    let parts = sheet["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    // Each the plate's top seen from above: 100 x 60 at 1:1, one outer
    // loop, placed apart on the sheet from its top left.
    for p in parts {
        assert_eq!(p["body"], "F1.b0");
        assert_eq!(p["body_name"], "Body1");
        assert_eq!(p["face"], "F1:top");
        assert_eq!(p["own_face"], false);
        assert_eq!(
            (p["outer"].as_u64(), p["inner"].as_u64()),
            (Some(1), Some(0))
        );
        assert!(near(number(&p["size"][0]), 100.0) && near(number(&p["size"][1]), 60.0));
        let [lo, hi] = sheet_box(p);
        assert!(
            near(hi[0] - lo[0], 100.0) && near(hi[1] - lo[1], 60.0),
            "{p}"
        );
        assert!(lo[0] >= 0.0 && hi[0] <= 600.0 && lo[1] >= 0.0 && hi[1] <= 300.0);
        assert_eq!(p["overlaps"], json!([]));
        let loops = p["loops"].as_array().unwrap();
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0]["outer"], true);
        assert_eq!(loops[0]["pieces"].as_array().unwrap().len(), 4);
    }
    assert_eq!(point(&parts[0]["place"]), [55.0, 265.0]);
    assert_eq!(sheet["warnings"], json!([]));
    assert_eq!(sheet["counts"].as_array().unwrap().len(), 1);
    assert_eq!(sheet["counts"][0]["count"], 2);
    assert_eq!(sheet["counts"][0]["parts"], json!([1, 2]));
    // Two views of the drawing query, each a group of its outline.
    let drawn = query(&doc, json!({"query": "drawing", "drawing": "D1"}));
    let views = drawn["sheets"][0]["views"].as_array().unwrap();
    assert_eq!(views.len(), 2);
    assert!(
        views
            .iter()
            .all(|v| v["type"] == "part" && v["label"] == "Body1")
    );
    let groups = drawn["sheets"][0]["groups"].as_array().unwrap();
    assert_eq!(groups.iter().filter(|g| g["class"] == "view").count(), 2);
    assert_eq!(groups[0]["id"], "part-1");
    // Another body added where it is asked for.
    let more = command(
        &mut doc,
        json!({"cmd": "add_cut_sheet_parts", "drawing": "D1", "bodies": [{"body": "F2.b0"}],
               "at": [400, 100]}),
    );
    assert_eq!(more["parts"], json!([3]));
    assert_eq!(undo_label(&doc), "Add part 3 to CutSheet1");
    let [lo, hi] = sheet_box(&part(&doc, 3));
    assert!(near(lo[0], 380.0) && near(hi[0], 420.0) && near(lo[1], 85.0) && near(hi[1], 115.0));
    // Refused: no body, no count, a face for several bodies.
    let error = refused(
        &mut doc,
        json!({"cmd": "add_cut_sheet_parts", "drawing": "D1", "bodies": ["F1.b0"], "count": 0}),
    );
    assert!(error.contains("1 to 1000"), "{error}");
    let error = refused(
        &mut doc,
        json!({"cmd": "add_cut_sheet_parts", "drawing": "D1", "bodies": ["F1.b0", "F2.b0"],
               "face": "F1:top"}),
    );
    assert!(error.contains("one body at a time"), "{error}");
}

fn point(value: &Value) -> [f64; 2] {
    [number(&value[0]), number(&value[1])]
}

#[test]
fn a_parameter_change_updates_the_outline() {
    let mut doc = plates();
    command(
        &mut doc,
        json!({"cmd": "add_cut_sheet", "bodies": ["F1.b0"]}),
    );
    let place = point(&part(&doc, 1)["place"]);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "len", "value": 140}),
    );
    let grown = part(&doc, 1);
    assert!(near(number(&grown["size"][0]), 140.0), "{grown}");
    // The part grows about its middle, which stays where it was put.
    let [lo, hi] = sheet_box(&grown);
    assert!(near(hi[0] - lo[0], 140.0) && near(hi[1] - lo[1], 60.0));
    assert!(near((lo[0] + hi[0]) / 2.0, place[0]) && near((lo[1] + hi[1]) / 2.0, place[1]));
    // Now it reaches out of the sheet's left edge: a warning.
    assert!(lo[0] < 0.0);
    assert_eq!(grown["off_sheet"], true);
    let warnings = cut_sheet(&doc)["warnings"].clone();
    assert_eq!(warnings, json!(["part 1 (Body1) leaves the sheet"]));
    command(&mut doc, json!({"cmd": "undo"}));
    assert!(near(number(&part(&doc, 1)["size"][0]), 100.0));
}

#[test]
fn parts_move_and_turn_and_overlaps_are_warned() {
    let mut doc = plates();
    command(
        &mut doc,
        json!({"cmd": "add_cut_sheet", "bodies": ["F1.b0", "F2.b0"]}),
    );
    // Body1 in the sheet's middle, where it can turn.
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": "D1", "part": 1, "place": [300, 150]}),
    );
    assert_eq!(undo_label(&doc), "Move part 1 of CutSheet1");
    // A quarter turn left: 60 wide and 100 high about its middle.
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": "D1", "part": 1, "turn": 90}),
    );
    assert_eq!(undo_label(&doc), "Turn part 1 of CutSheet1");
    let turned = part(&doc, 1);
    assert_eq!(number(&turned["rotation"]), 90.0);
    let [lo, hi] = sheet_box(&turned);
    assert!(
        near(hi[0] - lo[0], 60.0) && near(hi[1] - lo[1], 100.0),
        "{turned}"
    );
    // A typed angle, through the drawing's view command as the
    // application turns views.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "rotation": 450}),
    );
    assert_eq!(number(&part(&doc, 1)["rotation"]), 90.0);
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "rotation": 30}),
    );
    assert_eq!(number(&part(&doc, 1)["rotation"]), 30.0);
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 1, "scale": "1:2"}),
    );
    assert!(error.contains("moved and turned only"), "{error}");
    // Body2 moved onto Body1 by a drag: they overlap.
    let at = point(&part(&doc, 1)["place"]);
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": "D1", "part": 2, "place": [at[0] + 10.0, at[1]]}),
    );
    assert_eq!(undo_label(&doc), "Move part 2 of CutSheet1");
    let sheet = cut_sheet(&doc);
    assert_eq!(sheet["parts"][0]["overlaps"], json!([2]));
    assert_eq!(sheet["parts"][1]["overlaps"], json!([1]));
    assert_eq!(
        sheet["warnings"],
        json!(["part 1 (Body1) overlaps part 2 (Body2)"])
    );
    // Moved away by a drag of the view: no warning.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 2, "move": [200, 0]}),
    );
    assert_eq!(cut_sheet(&doc)["warnings"], json!([]));
    // Deleted as a view; back with undo.
    let deleted = command(
        &mut doc,
        json!({"cmd": "delete_drawing_view", "drawing": "D1", "view": 2}),
    );
    assert_eq!(deleted["deleted"], json!([2]));
    assert_eq!(undo_label(&doc), "Delete part 2 of CutSheet1");
    assert_eq!(cut_sheet(&doc)["parts"].as_array().unwrap().len(), 1);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(cut_sheet(&doc)["parts"].as_array().unwrap().len(), 2);
    let error = refused(
        &mut doc,
        json!({"cmd": "delete_cut_sheet_parts", "drawing": "D1", "parts": [9]}),
    );
    assert!(error.contains("no part 9"), "{error}");
}

#[test]
fn another_face_is_chosen_and_a_missing_body_is_marked() {
    let mut doc = plates();
    command(
        &mut doc,
        json!({"cmd": "add_cut_sheet", "bodies": ["F1.b0", "F2.b0"]}),
    );
    // The plate seen along its side face (+X): 60 wide and 3 high.
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": "D1", "part": 1, "face": "F1:side1"}),
    );
    let side = part(&doc, 1);
    assert_eq!(side["face"], "F1:side1");
    assert_eq!(side["own_face"], true);
    let size = [number(&side["size"][0]), number(&side["size"][1])];
    assert!(near(size[0], 60.0) && near(size[1], 3.0), "{side}");
    // Back to the largest.
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": "D1", "part": 1, "face": null}),
    );
    assert_eq!(part(&doc, 1)["face"], "F1:top");
    let error = refused(
        &mut doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": "D1", "part": 1, "face": "F2:top"}),
    );
    assert!(error.contains("no planar face"), "{error}");
    // Body2 deleted: its part stays, marked missing, and is not written.
    command(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    let gone = part(&doc, 2);
    assert_eq!(gone["missing"], true);
    assert!(
        gone["error"].as_str().unwrap().contains("is gone"),
        "{gone}"
    );
    let warnings = cut_sheet(&doc)["warnings"].clone();
    assert!(
        warnings[0]
            .as_str()
            .unwrap()
            .starts_with("part 2 (F2.b0) is missing"),
        "{warnings}"
    );
    let drawn = query(&doc, json!({"query": "drawing", "drawing": "D1"}));
    let view = &drawn["sheets"][0]["views"][1];
    assert_eq!(view["broken"], true);
    assert_eq!(view["missing"], true);
    let groups = drawn["sheets"][0]["groups"].as_array().unwrap();
    assert_eq!(groups.iter().filter(|g| g["class"] == "view").count(), 1);
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(part(&doc, 2)["missing"], false);
}

#[test]
fn a_cut_sheet_is_kept_in_the_design_file() {
    let mut doc = plates();
    command(
        &mut doc,
        json!({"cmd": "add_cut_sheet", "name": "Ribs", "width": 800, "bodies": ["F1.b0"], "count": 2}),
    );
    command(
        &mut doc,
        json!({"cmd": "edit_cut_sheet_part", "drawing": "Ribs", "part": 2, "rotation": -90}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    let kept = file["drawings"][0]["cut_sheet"].clone();
    assert_eq!(kept["width"], json!(800.0));
    assert_eq!(kept["parts"][1]["rotation"], json!(-90.0));
    assert!(kept["parts"][0].get("rotation").is_none(), "{kept}");
    let loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    loaded.kernel().box_faces.set(true);
    let sheet = query(&loaded, json!({"query": "cut_sheet", "drawing": "Ribs"}));
    assert_eq!(sheet["parts"].as_array().unwrap().len(), 2);
    assert_eq!(sheet["parts"][1]["rotation"], json!(-90.0));
}

#[test]
fn part_ids_in_the_file_leave_the_top_numbers_free() {
    // mitcad#944: a design file's part ids stay below the reserve, so the
    // next part's id exists.
    let top = LARGEST_U32_ID;
    let mut doc = plates();
    command(
        &mut doc,
        json!({"cmd": "add_cut_sheet", "bodies": ["F1.b0"]}),
    );
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    file["drawings"][0]["cut_sheet"]["parts"][0]["id"] = json!(top);
    let mut loaded = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    loaded.kernel().box_faces.set(true);
    // A design file is read unevaluated; its bodies come with recompute.
    command(&mut loaded, json!({"cmd": "recompute"}));
    let more = command(
        &mut loaded,
        json!({"cmd": "add_cut_sheet_parts", "drawing": "D1", "bodies": ["F2.b0"]}),
    );
    assert_eq!(more["parts"], json!([top + 1]));
    // One past the largest is refused as the file is read.
    file["drawings"][0]["cut_sheet"]["parts"][0]["id"] = json!(top + 1);
    let Err(error) = Document::from_json(&file.to_string(), MockKernel::default()) else {
        panic!("part {} was read", top + 1);
    };
    assert!(
        error
            .to_string()
            .contains("invalid cut-sheet part id '4293918720'"),
        "{error}"
    );
}

#[test]
fn the_dxf_and_svg_files_read_back_with_their_layers_and_sizes() {
    let mut doc = plates();
    command(
        &mut doc,
        json!({"cmd": "add_cut_sheet", "bodies": ["F1.b0", "F2.b0"]}),
    );
    let dir = std::env::temp_dir().join(format!("mitcad-cut-sheet-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dxf = dir.join("parts.dxf").to_string_lossy().into_owned();
    let written = command(
        &mut doc,
        json!({"cmd": "export_cut_sheet", "drawing": "D1", "path": dxf, "labels": true}),
    );
    assert_eq!(written["format"], "dxf");
    assert_eq!(
        (written["parts"].as_u64(), written["outer"].as_u64()),
        (Some(2), Some(2))
    );
    let back = mitcad_dxf::read_file(&dxf).unwrap();
    assert_eq!(back.version.as_deref(), Some("AC1015"));
    assert_eq!(back.units, mitcad_dxf::Units::Millimeters);
    for layer in ["CUT_OUTER", "CUT_INNER", "LABEL"] {
        assert!(back.layer(layer).is_some(), "{layer}");
    }
    let outer = back.on_layers(&["CUT_OUTER".to_owned()]);
    assert_eq!(outer.entities.len(), 8);
    // At 1:1: Body1's 100 x 60 and Body2's 40 x 30 side by side.
    let (min, max) = outer.bounds().unwrap();
    let [lo1, _] = sheet_box(&part(&doc, 1));
    let [_, hi2] = sheet_box(&part(&doc, 2));
    assert!(
        near(min.x, lo1[0]) && near(max.x, hi2[0]),
        "{min:?} {max:?}"
    );
    let labels: Vec<_> = back
        .entities
        .iter()
        .filter(|e| e.layer == "LABEL")
        .collect();
    assert_eq!(labels.len(), 2);
    // The SVG at the material's size, as the drawing's export too.
    let svg = dir.join("parts.svg").to_string_lossy().into_owned();
    command(
        &mut doc,
        json!({"cmd": "export_drawing", "drawing": "D1", "path": svg}),
    );
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.contains(r#"width="600mm" height="300mm" viewBox="0 0 600 300""#));
    assert!(text.contains(r#"<g id="CUT_OUTER""#) && text.contains(r#"<g id="CUT_INNER""#));
    assert_eq!(text.matches("<path").count(), 2);
    assert!(!text.contains(r#"<g id="LABEL""#));
    // Other formats are refused.
    let pdf = dir.join("parts.pdf").to_string_lossy().into_owned();
    let error = refused(
        &mut doc,
        json!({"cmd": "export_cut_sheet", "drawing": "D1", "path": pdf}),
    );
    assert!(error.contains("DXF or SVG"), "{error}");
    std::fs::remove_dir_all(&dir).ok();
}
