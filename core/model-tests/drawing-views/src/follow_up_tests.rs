// SPDX-License-Identifier: MIT
//! The follow-ups of drawings' tables (mitcad#718) on the mock kernel:
//! hole tables on section, detail and broken views; two parts lists on one
//! sheet and the balloons' numbers; a parts list of a section counting what
//! it shows; sorting by a hidden column and by item; a view square to an
//! inclined face as an orthographic one; a component's copied drawing with
//! tables, balloons and datum labels re-lettered; an unchanged letter on a
//! file where a letter is shared; the caches of bodies' geometry and holes;
//! tables in the PDF and DXF exports.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;
use mitcad_model_drawing::DrawingCache;

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

fn sheet_of(doc: &Document<MockKernel>, drawing: &str) -> Value {
    query(doc, json!({"query": "drawing", "drawing": drawing}))["sheets"][0].clone()
}

fn annotation_of(doc: &Document<MockKernel>, drawing: &str, id: u64) -> Value {
    sheet_of(doc, drawing)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id} in {drawing}"))
}

fn annotation(doc: &Document<MockKernel>, id: u64) -> Value {
    annotation_of(doc, "D1", id)
}

fn view_place(doc: &Document<MockKernel>, id: u64) -> [f64; 2] {
    let views = sheet_of(doc, "D1")["views"].clone();
    let view = views
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no view {id}"));
    [
        view["place"][0].as_f64().unwrap(),
        view["place"][1].as_f64().unwrap(),
    ]
}

fn added(doc: &mut Document<MockKernel>, cmd: Value) -> u64 {
    let result = command(doc, cmd);
    result["annotation"]
        .as_u64()
        .or_else(|| result["view"].as_u64())
        .unwrap_or_else(|| panic!("{result}"))
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

/// A 40 x 30 x `height` block at the origin (F2.b0) and a 20 x 10 x 10
/// block at x 50 (F4.b0), each with a hole of 6.6 through it from the
/// middle of its top; a drawing with a front view at 1:1 (its middle at
/// 100, 120) and the top view above it.
fn holed() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_holes.set(Some(6.6));
    command(
        &mut doc,
        json!([
            {"cmd": "add_parameter", "name": "height", "value": "20 mm"},
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": "height"}, "operation": "new_body"}},
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
fn hole_tables_list_what_sections_details_and_broken_views_show() {
    let mut doc = holed();
    doc.kernel().cuts_sections.set(true);
    // A section of the front view at z 5, seen from above: both holes run
    // through what is left.
    let section = added(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[50, 115], [150, 115]], "at": [320, 200]}),
    );
    let on_section = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": section,
               "type": "hole_table", "at": [250, 280]}),
    );
    let table = annotation(&doc, on_section);
    assert_eq!(table["broken"], false, "{table}");
    assert_eq!(
        table["label"], "A1 20 15 Ø6.6 THRU -; A2 60 5 Ø6.6 THRU -",
        "{table}"
    );

    // A detail of the top view round the first hole: that hole only.
    let top = view_place(&doc, 2);
    let detail = added(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "detail", "parent": 2,
               "center": [top[0] + 20.0, top[1] + 15.0], "radius": 25, "at": [340, 90]}),
    );
    let on_detail = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": detail,
               "type": "hole_table", "at": [250, 240], "table": {"origin": "model"}}),
    );
    let table = annotation(&doc, on_detail);
    assert_eq!(table["label"], "A1 20 15 Ø6.6 THRU -", "{table}");

    // The top view broken across x 45..65: the second hole lies in the
    // band taken out.
    let on_top = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2,
               "type": "hole_table", "at": [250, 200], "table": {"origin": "model"}}),
    );
    assert_eq!(
        annotation(&doc, on_top)["label"],
        "A1 20 15 Ø6.6 THRU -; A2 60 5 Ø6.6 THRU -"
    );
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": 2,
               "add_break": {"direction": "horizontal", "from": [top[0] + 45.0, top[1]],
                             "to": [top[0] + 65.0, top[1]]}}),
    );
    assert_eq!(
        annotation(&doc, on_top)["label"],
        "A1 20 15 Ø6.6 THRU -",
        "{}",
        annotation(&doc, on_top)
    );
}

#[test]
fn a_hole_table_sorts_by_a_column_it_does_not_show() {
    let mut doc = holed();
    let table = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "hole_table",
               "at": [250, 270],
               "table": {"columns": ["tag", "size"], "sort": "x", "descending": true}}),
    );
    let shown = annotation(&doc, table);
    assert_eq!(shown["label"], "A2 Ø6.6; A1 Ø6.6", "{shown}");
    assert_eq!(shown["columns"], json!(["tag", "size"]));
}

/// An assembly: Plate (a 40 x 30 x 5 block, F2.b0) placed twice (the
/// second 50 along X), Pin (a 10 x 10 x 20 block at y 40, F4.b0) once; a
/// drawing of the design's top view (its middle at 150, 150). Returns the
/// occurrences: Plate's two, Pin's.
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

#[test]
fn balloons_take_their_own_views_parts_list_first() {
    let (mut doc, [_, second, _]) = assembly();
    // Two more top views of the design; the first and the second list
    // their components, the second sorted the other way.
    for at in [[150, 70], [150, 230]] {
        command(
            &mut doc,
            json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
                   "at": at, "scale": "1:1"}),
        );
    }
    let first = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "parts_list",
               "at": [300, 280]}),
    );
    let other = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "parts_list",
               "at": [300, 200], "table": {"descending": true}}),
    );
    assert_eq!(annotation(&doc, first)["label"], "1 Pin x1; 2 Plate x2");
    assert_eq!(annotation(&doc, other)["label"], "1 Plate x2; 2 Pin x1");
    let plate_edge = edge_of(&doc, "F2.b0");
    let balloon = |doc: &mut Document<MockKernel>, view: u64| {
        added(
            doc,
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": view,
                   "type": "balloon",
                   "picks": [{"body": "F2.b0", "occurrence": second, "edge": plate_edge}]}),
        )
    };
    // Each view's balloon its own list's number; a view without a list
    // the sheet's first list's.
    let on_first = balloon(&mut doc, 1);
    let on_other = balloon(&mut doc, 2);
    let on_third = balloon(&mut doc, 3);
    assert_eq!(annotation(&doc, on_first)["label"], "2");
    assert_eq!(annotation(&doc, on_other)["label"], "1");
    assert_eq!(annotation(&doc, on_third)["label"], "2");
    // The first list gone: the other's numbers for all of them.
    command(
        &mut doc,
        json!({"cmd": "delete_drawing_annotation", "drawing": "D1", "annotation": first}),
    );
    assert_eq!(annotation(&doc, on_first)["label"], "1");
    assert_eq!(annotation(&doc, on_third)["label"], "1");
}

#[test]
fn a_parts_list_by_item_descending_lists_the_last_number_first() {
    let (mut doc, [_, second, _]) = assembly();
    let list = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "parts_list",
               "at": [300, 280], "table": {"sort": "item", "descending": true}}),
    );
    // Numbered by name, listed from the last number.
    assert_eq!(annotation(&doc, list)["label"], "2 Plate x2; 1 Pin x1");
    let edge = edge_of(&doc, "F2.b0");
    let balloon = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "balloon",
               "picks": [{"body": "F2.b0", "occurrence": second, "edge": edge}]}),
    );
    assert_eq!(annotation(&doc, balloon)["label"], "2");
}

#[test]
fn a_parts_list_of_a_section_counts_what_the_section_shows() {
    let (mut doc, [_, second, pin]) = assembly();
    doc.kernel().cuts_sections.set(true);
    // A section of the top view at y 35: Pin (y 40..50) lies wholly on the
    // side the plane takes away.
    let section = added(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[90, 160], [210, 160]], "at": [300, 230]}),
    );
    let list = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": section,
               "type": "parts_list", "at": [250, 120]}),
    );
    let shown = annotation(&doc, list);
    assert_eq!(shown["label"], "1 Plate x2", "{shown}");
    // A balloon on Pin, picked on the top view: no list of the sheet has
    // it.
    let pin_edge = edge_of(&doc, "F4.b0");
    let balloon = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "balloon",
               "picks": [{"body": "F4.b0", "occurrence": pin, "edge": pin_edge}]}),
    );
    assert_eq!(annotation(&doc, balloon)["broken"], true);
    // Plate's balloon takes its number from the section's list.
    let plate_edge = edge_of(&doc, "F2.b0");
    let on_plate = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "balloon",
               "picks": [{"body": "F2.b0", "occurrence": second, "edge": plate_edge}]}),
    );
    assert_eq!(annotation(&doc, on_plate)["label"], "1");
}

#[test]
fn a_view_square_to_an_inclined_face_is_orthographic() {
    let mut doc = holed();
    doc.kernel().lists_geometry.set(true);
    let s = std::f64::consts::FRAC_1_SQRT_2;
    // The first block's top face turned to face the eye of a view along
    // (0, -1, 1).
    doc.kernel().surfaces.borrow_mut().insert(
        format!("F2:end({R})"),
        SurfaceGeometry::Plane {
            origin: [0.0, 0.0, 20.0],
            normal: [0.0, -s, s],
        },
    );
    let square = added(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base",
               "direction": {"toward_eye": [0.0, -s, s], "x_axis": [1, 0, 0]},
               "at": [320, 200], "scale": "1:1"}),
    );
    let table = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": square,
               "type": "hole_table", "at": [250, 280]}),
    );
    assert_eq!(annotation(&doc, table)["broken"], false);
    // Square to no face: seen from an angle.
    let angled = added(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base",
               "direction": {"toward_eye": [0.0, -0.6, 0.8], "x_axis": [1, 0, 0]},
               "at": [320, 90], "scale": "1:1"}),
    );
    let error = refused(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": angled,
               "type": "hole_table", "at": [250, 240]}),
    );
    assert!(error.contains("from an angle"), "{error}");
}

#[test]
fn a_copied_components_drawing_keeps_its_tables_and_takes_free_datum_letters() {
    let mut doc = Document::new(MockKernel::default());
    doc.kernel().box_holes.set(Some(5.0));
    let made = command(
        &mut doc,
        json!({"cmd": "create_component", "name": "Bracket"}),
    );
    let occurrence = made["occurrence"].as_str().unwrap().to_owned();
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
        json!({"cmd": "add_drawing", "component": "Bracket", "size": "A3"}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
               "at": [150, 150], "scale": "1:1"}),
    );
    let edge = edge_of(&doc, "F2.b0");
    let pick = json!({"body": "F2.b0", "edge": edge});
    let holes = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "hole_table",
               "at": [300, 280]}),
    );
    let parts = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "parts_list",
               "at": [300, 220]}),
    );
    let balloon = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "balloon",
               "picks": [pick.clone()]}),
    );
    let datum = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "datum",
               "picks": [pick.clone()]}),
    );
    let frame = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1, "type": "frame",
               "picks": [pick],
               "frame": {"characteristic": "perpendicularity", "value": 0.05, "datums": ["A"]}}),
    );
    assert_eq!(annotation(&doc, datum)["letter"], "A");
    let hole_label = annotation(&doc, holes)["label"].clone();
    assert_ne!(hole_label, "no holes");

    // A copy of Bracket brings a copy of the drawing.
    command(
        &mut doc,
        json!({"cmd": "activate_component", "component": "C0"}),
    );
    command(
        &mut doc,
        json!({"cmd": "paste_new", "occurrence": occurrence}),
    );
    let drawings = query(&doc, json!({"query": "drawings"}));
    let copy = drawings[1]["drawing"].as_str().unwrap().to_owned();
    // Its tables and balloon work on the copy's body.
    let copied_holes = annotation_of(&doc, &copy, holes);
    assert_eq!(copied_holes["broken"], false, "{copied_holes}");
    assert_eq!(copied_holes["label"], hole_label);
    assert_eq!(annotation_of(&doc, &copy, parts)["broken"], false);
    let copied_balloon = annotation_of(&doc, &copy, balloon);
    assert_eq!(copied_balloon["broken"], false, "{copied_balloon}");
    assert_eq!(copied_balloon["label"], "1");
    // Its datum label takes a letter of its own; its frame follows it.
    let copied_datum = annotation_of(&doc, &copy, datum);
    assert_eq!(copied_datum["letter"], "B", "{copied_datum}");
    let copied_frame = annotation_of(&doc, &copy, frame);
    assert_eq!(
        copied_frame["frame"]["datums"],
        json!(["B"]),
        "{copied_frame}"
    );
    assert!(copied_frame.get("warning").is_none(), "{copied_frame}");
    // The original keeps its A.
    assert_eq!(annotation(&doc, datum)["letter"], "A");

    // A file where the two drawings share a letter (from before): an edit
    // that keeps the letter is no new claim; one to another drawing's
    // letter is refused.
    let mut file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    for a in file["drawings"][1]["sheets"][0]["views"][0]["annotations"]
        .as_array_mut()
        .unwrap()
    {
        if a["letter"] == "B" {
            a["letter"] = json!("A");
        }
        if a["frame"]["datums"] == json!(["B"]) {
            a["frame"]["datums"] = json!(["A"]);
        }
    }
    let mut shared = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    command(
        &mut shared,
        json!({"cmd": "edit_drawing_annotation", "drawing": copy, "annotation": datum,
               "letter": "A"}),
    );
    command(
        &mut shared,
        json!({"cmd": "edit_drawing_annotation", "drawing": copy, "annotation": datum,
               "letter": "C"}),
    );
    let error = refused(
        &mut shared,
        json!({"cmd": "edit_drawing_annotation", "drawing": copy, "annotation": datum,
               "letter": "A"}),
    );
    assert!(error.contains("datum A exists"), "{error}");
}

#[test]
fn bodies_geometry_and_holes_are_kept_once_per_body() {
    let mut doc = holed();
    let table = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "hole_table",
               "at": [250, 270]}),
    );
    assert_ne!(annotation(&doc, table)["label"], "no holes");
    // The holes found once: drawn again, the kernel is not asked.
    let found = doc.kernel().count("round_holes");
    sheet_of(&doc, "D1");
    assert_eq!(doc.kernel().count("round_holes"), found);
    // A body's geometry kept once per body: a new version replaces the
    // old one.
    let kept = doc.drawing_cache().geometries();
    assert_eq!(kept, 2, "one per body");
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 35}),
    );
    sheet_of(&doc, "D1");
    assert_eq!(doc.drawing_cache().geometries(), kept);
    // The changed body's holes found again.
    assert!(doc.kernel().count("round_holes") > found);
}

#[test]
fn a_failed_hole_search_is_tried_again_and_names_the_body_as_it_is() {
    // mitcad#722: the holes' cache kept a failure with its warning, so it
    // was not tried again until the body changed and the warning kept a
    // renamed body's old name.
    let mut doc = holed();
    let table = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "hole_table",
               "at": [250, 270]}),
    );
    assert_ne!(annotation(&doc, table)["label"], "no holes");
    let warnings = |doc: &Document<MockKernel>| sheet_of(doc, "D1")["warnings"].to_string();
    // The block changed while the kernel fails: a warning, no holes.
    doc.kernel().fail.borrow_mut().insert("round_holes");
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "height", "value": 35}),
    );
    assert!(
        warnings(&doc).contains("round_holes failed"),
        "{}",
        warnings(&doc)
    );
    // Renamed: the warning names it as it is now.
    command(
        &mut doc,
        json!({"cmd": "rename_body", "uid": "F2.b0", "name": "Bracket"}),
    );
    let now = warnings(&doc);
    assert!(now.contains("the holes of Bracket"), "{now}");
    // The kernel works again: the holes are found without a change.
    doc.kernel().fail.borrow_mut().remove("round_holes");
    assert!(!warnings(&doc).contains("round_holes failed"));
    let label = annotation(&doc, table)["label"].clone();
    assert_ne!(label, "no holes", "{label}");
}

#[test]
fn a_sections_bodies_are_kept_apart_from_the_views_projections() {
    // mitcad#722: what a parts list counts on a section was found from
    // each body's own projection through the views' cache, projected
    // again on every draw and pushing the views' projections out.
    let (mut doc, _) = assembly();
    doc.kernel().cuts_sections.set(true);
    let section = added(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[90, 160], [210, 160]], "at": [300, 230]}),
    );
    sheet_of(&doc, "D1");
    let views = doc.drawing_cache().projections();
    assert_eq!(doc.drawing_cache().body_views(), 0);
    let list = added(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": section,
               "type": "parts_list", "at": [250, 120]}),
    );
    assert_eq!(annotation(&doc, list)["label"], "1 Plate x2");
    // One answer per body of the section, none among the views'.
    assert_eq!(doc.drawing_cache().body_views(), 3);
    assert_eq!(doc.drawing_cache().projections(), views);
    // Drawn again: nothing is projected.
    let projected = doc.kernel().count("project_view");
    sheet_of(&doc, "D1");
    assert_eq!(doc.kernel().count("project_view"), projected);
    assert_eq!(annotation(&doc, list)["label"], "1 Plate x2");
}

#[test]
fn tables_are_written_to_pdf_and_dxf() {
    let mut doc = holed();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "hole_table",
               "at": [250, 270]}),
    );
    command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 2, "type": "parts_list",
               "at": [250, 200]}),
    );
    let dir = mitcad_model::testing::unique_temp_dir("table-exports");
    for name in ["tables.pdf", "tables.dxf"] {
        let path = dir.join(name);
        command(
            &mut doc,
            json!({"cmd": "export_drawing", "drawing": "D1", "path": path.to_string_lossy()}),
        );
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        for expected in ["BILL OF MATERIALS", "TAG", "A1", "A2", "THRU"] {
            assert!(text.contains(expected), "{name}: {expected}");
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}
